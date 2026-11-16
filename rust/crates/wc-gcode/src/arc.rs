//! Arcs the firmware cannot run as written (spec/firmware.md §9): R, which 51.27 ignores and 51.29x reads unsigned,
//! and G91, which every version's G2/G3 ignores. The interpreter is followed so such arcs go out as I/J, absolute.

use std::ops::Range;

use crate::{Positioning, Units};

/// How far a radius may fall short of half the chord and still be read as a half circle.
pub const RADIUS_TOLERANCE_MM: f64 = 0.05;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArcProblem {
    RadiusTooSmall,
    /// No X/Y change: every circle through the point has that radius.
    FullCircle,
    /// G18 or G19, which the firmware ignores, cutting the arc in XY.
    Plane,
    /// The firmware reads an arc's Z as absolute even under G91.
    RelativeZ,
    /// Neither a commanded position nor a status report says where the arc starts.
    StartUnknown,
}

impl ArcProblem {
    pub fn title(&self) -> &'static str {
        match self {
            ArcProblem::RadiusTooSmall | ArcProblem::FullCircle => "Impossible Arc",
            ArcProblem::Plane | ArcProblem::RelativeZ => "Unsupported Arc",
            ArcProblem::StartUnknown => "Arc Start Unknown",
        }
    }

    pub fn reason(&self) -> &'static str {
        match self {
            ArcProblem::RadiusTooSmall => "is an arc whose radius (R) is shorter than half the distance to its end.",
            ArcProblem::FullCircle => "is an arc with a radius (R) that ends where it starts, so no circle is defined.",
            ArcProblem::Plane => {
                "is an arc with a radius (R) in the G18 or G19 plane, which the controller cannot cut."
            }
            ArcProblem::RelativeZ => {
                "is a relative (G91) arc with a Z move; the controller takes an arc's Z as absolute."
            }
            ArcProblem::StartUnknown => {
                "is an arc the controller needs rewritten (a radius as I and J, or G91 as absolute X and Y), and \
                 WebControl does not know where the sled is to rewrite it."
            }
        }
    }

    pub fn advice(&self) -> &'static str {
        match self {
            ArcProblem::StartUnknown => "Resume returns the sled to the file's position and tries the line again.",
            _ => "Resume stops here again: stop the job, fix the file and load it again.",
        }
    }
}

/// The centre of the arc from `start` to `end` with `radius`; a negative radius is the arc over 180°.
pub fn centre_from_radius(
    start: [f64; 2],
    end: [f64; 2],
    radius: f64,
    clockwise: bool,
    tolerance: f64,
) -> Result<[f64; 2], ArcProblem> {
    let (dx, dy) = (end[0] - start[0], end[1] - start[1]);
    let chord = dx.hypot(dy);
    if chord < 1e-9 {
        return Err(ArcProblem::FullCircle);
    }
    if radius.is_nan() || radius.abs() < chord / 2.0 - tolerance {
        return Err(ArcProblem::RadiusTooSmall);
    }
    let height = (radius.powi(2) - (chord / 2.0).powi(2)).max(0.0).sqrt();
    let side = if clockwise { -1.0 } else { 1.0 } * radius.signum();
    let (left_x, left_y) = (-dy / chord, dx / chord);
    Ok([(start[0] + end[0]) / 2.0 + side * height * left_x, (start[1] + end[1]) / 2.0 + side * height * left_y])
}

/// The firmware's `findEndOfNumber` and `toFloat`: the digits and punctuation after the letter, `None` without it.
fn firmware_value(segment: &str, letter: char) -> Option<f64> {
    word_range(segment, letter).map(|range| leading_float(&segment[range.start + 1..range.end]))
}

fn word_range(segment: &str, letter: char) -> Option<Range<usize>> {
    let at = segment.find(letter)?;
    let run = segment[at + 1..].bytes().take_while(|b| b.is_ascii_digit() || b.is_ascii_punctuation()).count();
    Some(at..at + 1 + run)
}

/// `atof`: the longest leading `[+-]digits[.digits]`, or 0.
fn leading_float(text: &str) -> f64 {
    let bytes = text.as_bytes();
    let digits =
        |from: usize| bytes.get(from..).map_or(0, |rest| rest.iter().take_while(|b| b.is_ascii_digit()).count());
    let point = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let point = point + digits(point);
    let fraction = if bytes.get(point) == Some(&b'.') { digits(point + 1) } else { 0 };
    let end = if fraction > 0 { point + 1 + fraction } else { point };
    text[..end].parse().unwrap_or(0.0)
}

fn four(value: f64) -> String {
    let text = format!("{value:.4}");
    if text == "-0.0000" { "0.0000".into() } else { text }
}

fn mm_per(units: Units) -> f64 {
    if units == Units::Inches { 25.4 } else { 1.0 }
}

type Edit = (Range<usize>, String);

/// The modal state of the controller's interpreter (`interpretCommandString`), as the lines written to it leave it.
#[derive(Debug, Clone, PartialEq)]
pub struct Interpreter {
    units: Option<Units>,
    /// The firmware's, which G91.1 also sets, since it truncates the code to 91.
    positioning: Positioning,
    /// A real G91: the file means its arcs' X and Y as offsets.
    relative_arcs: bool,
    plane: u32,
    /// `sys.lastGCommand`, which a line without a G word repeats; zero after a boot.
    motion: u32,
    xy_mm: Option<[f64; 2]>,
}

impl Default for Interpreter {
    fn default() -> Interpreter {
        Interpreter::new()
    }
}

impl Interpreter {
    pub fn new() -> Interpreter {
        let positioning = Positioning::Absolute;
        Interpreter { units: None, positioning, relative_arcs: false, plane: 17, motion: 0, xy_mm: None }
    }

    pub fn units(&self) -> Option<Units> {
        self.units
    }

    pub fn xy_mm(&self) -> Option<[f64; 2]> {
        self.xy_mm
    }

    pub fn set_xy_mm(&mut self, xy_mm: Option<[f64; 2]>) {
        self.xy_mm = xy_mm;
    }

    /// Follows a written line; true when it may move the sled or changes units.
    pub fn written(&mut self, line: &str) -> bool {
        self.walk(line, false).is_ok_and(|(_, moved)| moved)
    }

    /// `line` with every R arc given as I and J and every G91 arc's X and Y as absolute; unchanged without one.
    pub fn convert(&self, line: &str) -> Result<String, ArcProblem> {
        self.clone().walk(line, true).map(|(line, _)| line)
    }

    fn walk(&mut self, line: &str, convert: bool) -> Result<(String, bool), ArcProblem> {
        let upper = line.to_ascii_uppercase();
        let command = upper.trim();
        if command.starts_with('B') || command.starts_with('$') && command != "$$" {
            self.xy_mm = None;
            return Ok((line.to_string(), true));
        }
        let starts: Vec<usize> = upper.match_indices(['G', 'M']).map(|(at, _)| at).collect();
        // Without a G or M the whole line runs as the modal G.
        let starts = if starts.is_empty() { vec![0] } else { starts };
        let ends = starts.iter().skip(1).copied().chain([upper.len()]);
        let segments = starts.iter().zip(ends).map(|(&from, to)| from..to);
        let mut edits: Vec<Edit> = Vec::new();
        let mut moved = false;
        segments.filter(|range| !upper[range.clone()].starts_with('M')).try_for_each(|range| {
            let (found, segment_moved) = self.segment(&upper[range.clone()], convert)?;
            moved |= segment_moved;
            edits.extend(found.into_iter().map(|(at, text)| (at.start + range.start..at.end + range.start, text)));
            Ok(())
        })?;
        let mut out = String::with_capacity(line.len() + 24);
        let end = edits.iter().fold(0, |from, (range, text)| {
            out.push_str(&line[from..range.start]);
            out.push_str(text);
            range.end
        });
        out.push_str(&line[end..]);
        Ok((out, moved))
    }

    /// `executeGcodeLine` for one segment: the edits that rewrite its arc, and whether it may move the sled.
    fn segment(&mut self, segment: &str, convert: bool) -> Result<(Vec<Edit>, bool), ArcProblem> {
        let number = firmware_value(segment, 'G');
        let code = number.map_or(self.motion, |code| code as u32);
        let mut moved = false;
        let mut edits = Vec::new();
        match code {
            0..=3 => {
                let arc = code >= 2;
                let radius = firmware_value(segment, 'R').filter(|_| arc && !segment.contains(['I', 'J']));
                let rewrite = convert && (radius.is_some() || arc && self.relative_arcs);
                // The firmware's G2 and G3 ignore G91, so a rewritten relative arc is written absolute.
                let relative =
                    if arc { rewrite && self.relative_arcs } else { self.positioning == Positioning::Relative };
                let end = self.end_of(segment, relative);
                if rewrite {
                    edits = self.arc_edits(segment, code == 2, radius, end)?;
                }
                self.motion = code;
                self.xy_mm = end;
                moved = segment.contains(['X', 'Y']);
            }
            17..=19 => self.plane = code,
            20 | 21 => {
                let units = Some(if code == 20 { Units::Inches } else { Units::Mm });
                moved = units != self.units;
                self.units = units;
            }
            90 | 91 => {
                self.positioning = if code == 90 { Positioning::Absolute } else { Positioning::Relative };
                if number.is_some_and(|number| number.fract() == 0.0) {
                    self.relative_arcs = code == 91;
                }
            }
            _ => {}
        }
        Ok((edits, moved))
    }

    /// Where a move ends, in mm; `None` when the start or the units it needs are unknown.
    fn end_of(&self, segment: &str, relative: bool) -> Option<[f64; 2]> {
        let (x, y) = (firmware_value(segment, 'X'), firmware_value(segment, 'Y'));
        if x.is_none() && y.is_none() {
            return self.xy_mm;
        }
        let scale = mm_per(self.units?);
        let start = self.xy_mm;
        let axis = |value: Option<f64>, index: usize| match value {
            Some(value) if relative => start.map(|start| start[index] + value * scale),
            Some(value) => Some(value * scale),
            None => start.map(|start| start[index]),
        };
        Some([axis(x, 0)?, axis(y, 1)?])
    }

    /// An R given as I and J, and under G91 the X and Y given as where the arc ends.
    fn arc_edits(
        &self,
        segment: &str,
        clockwise: bool,
        radius: Option<f64>,
        end: Option<[f64; 2]>,
    ) -> Result<Vec<Edit>, ArcProblem> {
        let relative = self.relative_arcs;
        if radius.is_some() && self.plane != 17 {
            return Err(ArcProblem::Plane);
        }
        if relative && segment.contains('Z') {
            return Err(ArcProblem::RelativeZ);
        }
        let needs_start = radius.is_some() || relative && segment.contains(['X', 'Y']);
        let (Some(units), Some(start), Some(end)) = (self.units, self.xy_mm, end) else {
            return if needs_start { Err(ArcProblem::StartUnknown) } else { Ok(Vec::new()) };
        };
        let scale = mm_per(units);
        let mut edits = Vec::new();
        if let Some(radius) = radius {
            let centre = centre_from_radius(start, end, radius * scale, clockwise, RADIUS_TOLERANCE_MM)?;
            let offset = |axis: usize| four((centre[axis] - start[axis]) / scale);
            let r = word_range(segment, 'R').expect("an R word");
            edits.push((r, format!("I{} J{}", offset(0), offset(1))));
        }
        if relative {
            edits.extend([('X', 0), ('Y', 1)].into_iter().filter_map(|(letter, axis)| {
                word_range(segment, letter).map(|range| (range, format!("{letter}{}", four(end[axis] / scale))))
            }));
        }
        edits.sort_by_key(|(range, _)| range.start);
        Ok(edits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f64; 2], b: [f64; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9
    }

    fn after(lines: &[&str]) -> Interpreter {
        let mut interpreter = Interpreter::new();
        lines.iter().for_each(|line| {
            interpreter.written(line);
        });
        interpreter
    }

    #[test]
    fn a_quarter_circle_centres_on_the_side_its_direction_needs() {
        let centre = |clockwise| centre_from_radius([10.0, 10.0], [20.0, 0.0], 10.0, clockwise, 0.0).unwrap();
        assert!(close(centre(true), [10.0, 0.0]));
        assert!(close(centre(false), [20.0, 10.0]));
    }

    #[test]
    fn a_negative_radius_is_the_arc_over_180_degrees() {
        let centre = |radius| centre_from_radius([20.0, 0.0], [10.0, -10.0], radius, false, 0.0).unwrap();
        assert!(close(centre(-10.0), [10.0, 0.0]));
        assert!(close(centre(10.0), [20.0, -10.0]));
    }

    #[test]
    fn a_radius_short_of_half_the_chord_is_refused_beyond_the_tolerance() {
        let half = centre_from_radius([0.0, 0.0], [10.04, 0.0], 5.0, true, RADIUS_TOLERANCE_MM).unwrap();
        assert!(close(half, [5.02, 0.0]));
        assert_eq!(centre_from_radius([0.0, 0.0], [10.0, 0.0], 4.9, true, 0.05), Err(ArcProblem::RadiusTooSmall));
        assert_eq!(centre_from_radius([0.0, 0.0], [10.0, 0.0], f64::NAN, true, 0.05), Err(ArcProblem::RadiusTooSmall));
        assert_eq!(centre_from_radius([3.0, 4.0], [3.0, 4.0], 5.0, true, 0.05), Err(ArcProblem::FullCircle));
    }

    #[test]
    fn an_r_arc_is_written_with_i_and_j_in_both_directions() {
        let at = after(&["G21 ", "G90 ", "G0 X10 Y10 "]);
        assert_eq!(at.convert("G2 X20 Y0 R10 F300 ").unwrap(), "G2 X20 Y0 I0.0000 J-10.0000 F300 ");
        assert_eq!(at.convert("G03 X20 Y0 R10").unwrap(), "G03 X20 Y0 I10.0000 J0.0000");
        let long = after(&["G21 ", "G0 X20 Y0 "]);
        assert_eq!(long.convert("G3 X10 Y-10 R-10 ").unwrap(), "G3 X10 Y-10 I-10.0000 J0.0000 ");
    }

    #[test]
    fn inches_are_converted_in_inches() {
        let mut at = after(&["G20 ", "G0 X1 Y1 "]);
        assert_eq!(at.convert("G2 X1.5 Y0.5 R0.5 ").unwrap(), "G2 X1.5 Y0.5 I0.0000 J-0.5000 ");
        assert!(at.written("G2 X1.5 Y0.5 I0 J-0.5 "));
        assert!(close(at.xy_mm().unwrap(), [38.1, 12.7]));
    }

    #[test]
    fn a_relative_r_arc_is_written_absolute_because_the_firmware_reads_arcs_so() {
        let at = after(&["G21 ", "G0 X10 Y10 ", "G91 "]);
        assert_eq!(at.convert("G2 X10 Y-10 R10 ").unwrap(), "G2 X20.0000 Y0.0000 I0.0000 J-10.0000 ");
        assert_eq!(at.convert("G3 Y-20 R-10 ").unwrap(), "G3 Y-10.0000 I0.0000 J-10.0000 ");
        let mut relative = at.clone();
        relative.written("G1 X1 Y1 ");
        assert!(close(relative.xy_mm().unwrap(), [11.0, 11.0]));
        relative.written("G2 X0 Y0 I-5 J-5 ");
        assert!(close(relative.xy_mm().unwrap(), [0.0, 0.0]));
    }

    #[test]
    fn a_relative_i_j_arc_is_written_absolute_and_one_with_z_is_refused() {
        let at = after(&["G21 ", "G0 X10 Y10 ", "G91 "]);
        assert_eq!(at.convert("G2 X10 Y-10 I0 J-10 F300 ").unwrap(), "G2 X20.0000 Y0.0000 I0 J-10 F300 ");
        assert_eq!(at.convert("g3 y5 i0 j2.5").unwrap(), "g3 Y15.0000 i0 j2.5");
        assert_eq!(at.convert("G2 I5 J0 ").unwrap(), "G2 I5 J0 ");
        assert_eq!(at.convert("G1 X5 ").unwrap(), "G1 X5 ");
        assert_eq!(at.convert("G2 X1 Y1 I1 J0 Z-1 ").unwrap_err(), ArcProblem::RelativeZ);
        assert_eq!(after(&["G21 ", "G91 "]).convert("G2 X1 I1 J0 ").unwrap_err(), ArcProblem::StartUnknown);
        assert_eq!(after(&["G21 ", "G91 "]).convert("G2 I1 J0 ").unwrap(), "G2 I1 J0 ");
        let mut absolute = at.clone();
        absolute.written("G90 ");
        assert_eq!(absolute.convert("G2 X10 Y-10 I0 J-10 Z-1 ").unwrap(), "G2 X10 Y-10 I0 J-10 Z-1 ");
    }

    #[test]
    fn g91_1_moves_lines_relative_as_the_firmware_does_but_leaves_arcs_absolute() {
        let mut at = after(&["G21 ", "G0 X10 Y10 ", "G91.1 "]);
        assert_eq!(at.convert("G2 X20 Y0 I0 J-10 Z-1 ").unwrap(), "G2 X20 Y0 I0 J-10 Z-1 ");
        assert_eq!(at.convert("G2 X20 Y0 R10 ").unwrap(), "G2 X20 Y0 I0.0000 J-10.0000 ");
        at.written("G1 X1 ");
        assert!(close(at.xy_mm().unwrap(), [11.0, 10.0]));
        let mut real = after(&["G21 ", "G0 X0 Y0 ", "G91 ", "G90.1 "]);
        assert_eq!(real.convert("G2 X2 I1 J0 ").unwrap(), "G2 X2.0000 I1 J0 ");
        real.written("G90 ");
        assert_eq!(real.convert("G2 X2 I1 J0 ").unwrap(), "G2 X2 I1 J0 ");
    }

    #[test]
    fn lower_case_words_and_modal_arcs_are_converted() {
        let at = after(&["g21", "g0 x10 y10"]);
        assert_eq!(at.convert("g2 x20 y0 r10").unwrap(), "g2 x20 y0 I0.0000 J-10.0000");
        let modal = after(&["G21 ", "G0 X10 Y10 ", "G2 X20 Y0 I0 J-10 "]);
        assert_eq!(modal.convert("X10 Y-10 R10 ").unwrap(), "X10 Y-10 I-10.0000 J0.0000 ");
        let linear = after(&["G21 ", "G0 X10 Y10 ", "G1 X20 "]);
        assert_eq!(linear.convert("X30 R10 ").unwrap(), "X30 R10 ");
    }

    #[test]
    fn each_command_on_a_line_is_read_in_turn() {
        let at = after(&["G21 "]);
        assert_eq!(
            at.convert("G0 X10 Y10 G2 X20 Y0 R10 M3 G3 X30 Y10 R10 ").unwrap(),
            "G0 X10 Y10 G2 X20 Y0 I0.0000 J-10.0000 M3 G3 X30 Y10 I0.0000 J10.0000 "
        );
        assert_eq!(after(&["G0 X1 Y1 "]).convert("G20 G2 X2 Y2 R1 ").unwrap_err(), ArcProblem::StartUnknown);
        assert_eq!(after(&["G20 ", "G0 X1 Y1 "]).convert("T2 G2 X2 Y0 R1 ").unwrap(), "T2 G2 X2 Y0 I0.0000 J-1.0000 ");
    }

    #[test]
    fn lines_without_an_r_arc_are_left_alone() {
        let at = after(&["G21 ", "G0 X0 Y0 "]);
        ["G2 X10 Y0 I5 J0 R3 ", "G1 X10 R3 ", "B14 R1 ", "$13=R ", "M3 R1 ", "G4 P1 "]
            .iter()
            .for_each(|line| assert_eq!(at.convert(line).unwrap(), *line));
    }

    #[test]
    fn r_arcs_the_controller_cannot_cut_are_refused() {
        let at = after(&["G21 ", "G0 X10 Y10 "]);
        let refused = |at: &Interpreter, line| at.convert(line).unwrap_err();
        assert_eq!(refused(&at, "G2 X30 Y10 R5 "), ArcProblem::RadiusTooSmall);
        assert_eq!(refused(&at, "G2 X10 Y10 R5 "), ArcProblem::FullCircle);
        assert_eq!(refused(&at, "G2 R5 "), ArcProblem::FullCircle);
        assert_eq!(refused(&at, "G18 G2 X20 Y0 R10 "), ArcProblem::Plane);
        assert_eq!(refused(&after(&["G21 ", "G0 X0 Y0 ", "G19 ", "G17 "]), "G2 X2 R0.9"), ArcProblem::RadiusTooSmall);
        assert_eq!(refused(&after(&["G21 ", "G0 X0 Y0 ", "G91 "]), "G2 X2 R1 Z-1"), ArcProblem::RelativeZ);
        assert_eq!(refused(&after(&["G21 "]), "G2 X20 Y0 R10 "), ArcProblem::StartUnknown);
        assert_eq!(refused(&after(&["G0 X1 Y1 "]), "G2 X2 Y0 R1 "), ArcProblem::StartUnknown);
    }

    #[test]
    fn the_position_follows_what_the_firmware_does_with_each_line() {
        let mut at = after(&["G21 ", "G0 X10 Y10 "]);
        assert!(!at.written("G1 Z-1 F100 "));
        assert!(!at.written("G90 X99 "));
        assert!(close(at.xy_mm().unwrap(), [10.0, 10.0]));
        assert!(at.written("G91 G2 X5 Y5 I1 J0 "));
        assert!(close(at.xy_mm().unwrap(), [5.0, 5.0]));
        assert!(at.written("B08 "));
        assert_eq!(at.xy_mm(), None);
        assert!(at.written("G1 X2 "));
        assert_eq!(at.xy_mm(), None);
        at.set_xy_mm(Some([1.0, 2.0]));
        assert!(!at.written("$$"));
        assert!(at.written("$13=1 "));
        assert_eq!(at.xy_mm(), None);
        assert!(at.written("G20 ") && !at.written("G20 "));
        assert_eq!(at.units(), Some(Units::Inches));
    }

    #[test]
    fn numbers_are_read_as_the_firmware_reads_them() {
        assert_eq!(firmware_value("G2 X-1.5Y2", 'X'), Some(-1.5));
        assert_eq!(firmware_value("G2 X 10", 'X'), Some(0.0));
        assert_eq!(firmware_value("G2 X1.2.3", 'X'), Some(1.2));
        assert_eq!(firmware_value("G2 X.5", 'X'), Some(0.5));
        assert_eq!(firmware_value("G2 X5.", 'X'), Some(5.0));
        assert_eq!(firmware_value("G2 X-", 'X'), Some(0.0));
        assert_eq!(firmware_value("G2", 'X'), None);
        assert_eq!(firmware_value("G38.2 Z-1", 'G'), Some(38.2));
        assert_eq!(four(-0.00001), "0.0000");
    }

    #[test]
    fn every_problem_has_a_title_a_reason_and_advice() {
        let all = [
            ArcProblem::RadiusTooSmall,
            ArcProblem::FullCircle,
            ArcProblem::Plane,
            ArcProblem::RelativeZ,
            ArcProblem::StartUnknown,
        ];
        all.iter().for_each(|problem| {
            assert!(!problem.title().is_empty() && problem.reason().starts_with("is "), "{problem:?}");
        });
        assert!(ArcProblem::StartUnknown.advice().starts_with("Resume returns"));
        assert!(ArcProblem::Plane.advice().starts_with("Resume stops here again"));
        assert_eq!(Interpreter::default(), Interpreter::new());
    }
}
