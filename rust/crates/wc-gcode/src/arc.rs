//! Arcs the firmware cannot run as written (spec/firmware.md §9): R, which 51.27 ignores and 51.29x reads unsigned;
//! G91, which every version's G2/G3 ignores; and G90.1 and G91.1, which it truncates to G90 and G91. The interpreter
//! is followed so such arcs go out as I and J offsets with absolute X and Y, and G90.1 and G91.1 never go out.

use std::ops::Range;

use crate::{Positioning, Units, words};

/// How far a radius may fall short of half the chord and still be read as a half circle.
pub const RADIUS_TOLERANCE_MM: f64 = 0.05;
/// How far an arc rewritten from a start a status report gave may be cut from where it belongs.
pub const ARC_START_TOLERANCE_MM: f64 = 0.05;

/// What an arc's I and J measure from: its start (G91.1, all the firmware knows) or the origin (G90.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArcCentres {
    #[default]
    Offsets,
    Absolute,
}

impl ArcCentres {
    pub fn word(self) -> &'static str {
        match self {
            ArcCentres::Offsets => "G91.1 ",
            ArcCentres::Absolute => "G90.1 ",
        }
    }

    /// The mode a G number sets, if it is G90.1 or G91.1.
    pub fn set_by(code: f64) -> Option<ArcCentres> {
        [(90.1, ArcCentres::Absolute), (91.1, ArcCentres::Offsets)]
            .into_iter()
            .find_map(|(number, centres)| (code == number).then_some(centres))
    }
}

/// The console note for a file that sets arc-centre modes, naming the codes it uses.
pub fn arc_centre_note(lines: &[String]) -> Option<String> {
    let modes: Vec<ArcCentres> =
        lines.iter().flat_map(|line| words::numbers('G', &words::code(line))).filter_map(ArcCentres::set_by).collect();
    let codes = match (modes.contains(&ArcCentres::Offsets), modes.contains(&ArcCentres::Absolute)) {
        (true, true) => "G91.1 and G90.1",
        (true, false) => "G91.1",
        (false, true) => "G90.1",
        (false, false) => return None,
    };
    Some(format!("This file uses {codes}; WebControl converts it for the Maslow firmware."))
}

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
    /// Only a status report says where the arc starts, and its two decimals could move the arc too far.
    StartUncertain,
}

impl ArcProblem {
    pub fn title(&self) -> &'static str {
        match self {
            ArcProblem::RadiusTooSmall | ArcProblem::FullCircle => "Impossible Arc",
            ArcProblem::Plane | ArcProblem::RelativeZ => "Unsupported Arc",
            ArcProblem::StartUnknown => "Arc Start Unknown",
            ArcProblem::StartUncertain => "Arc Start Uncertain",
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
                "is an arc the controller needs rewritten (a radius or an absolute centre as I and J offsets, or G91 \
                 as absolute X and Y), and WebControl does not know where the sled is to rewrite it."
            }
            ArcProblem::StartUncertain => {
                "is an arc the controller needs rewritten, and WebControl knows where the sled is only to the status \
                 report's two decimals, which could cut this arc more than 0.05 mm from where it belongs. An \
                 absolute move (G0 X Y) first gives it an exact start."
            }
        }
    }

    pub fn advice(&self) -> &'static str {
        match self {
            ArcProblem::StartUnknown | ArcProblem::StartUncertain => {
                "Resume returns the sled to the file's position and tries the line again."
            }
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

/// The firmware's segments of an upper-cased line: split before each G and M, or the whole line without either.
fn segments(upper: &str) -> Vec<Range<usize>> {
    let starts: Vec<usize> = upper.match_indices(['G', 'M']).map(|(at, _)| at).collect();
    let starts = if starts.is_empty() { vec![0] } else { starts };
    let ends = starts.iter().skip(1).copied().chain([upper.len()]);
    starts.iter().zip(ends).map(|(&from, to)| from..to).collect()
}

/// The G number of each of the line's segments, as the firmware reads them.
pub(crate) fn g_numbers(line: &str) -> Vec<f64> {
    let upper = line.to_ascii_uppercase();
    segments(&upper).into_iter().filter_map(|range| firmware_value(&upper[range], 'G')).collect()
}

type Edit = (Range<usize>, String);

#[derive(Debug, Clone, Copy)]
enum Centre {
    Radius(f64),
    Offset([f64; 2]),
    Absolute([f64; 2]),
}

/// An arc as the file means it, in mm, from wherever it starts.
#[derive(Debug, Clone, Copy)]
struct Geometry {
    clockwise: bool,
    relative: bool,
    end: [Option<f64>; 2],
    centre: Centre,
}

impl Geometry {
    /// The centre and the end the file means from `start`.
    fn from(&self, start: [f64; 2]) -> Result<([f64; 2], [f64; 2]), ArcProblem> {
        let end = [0, 1].map(|axis| match self.end[axis] {
            Some(value) if self.relative => start[axis] + value,
            Some(value) => value,
            None => start[axis],
        });
        let centre = match self.centre {
            Centre::Radius(radius) => centre_from_radius(start, end, radius, self.clockwise, RADIUS_TOLERANCE_MM)?,
            Centre::Offset(offset) => [start[0] + offset[0], start[1] + offset[1]],
            Centre::Absolute(centre) => centre,
        };
        Ok((centre, end))
    }

    /// How far the arc written from `start` may be cut from where the file means, with the sled anywhere within
    /// `spread` of `start`: the firmware offsets the centre from where it really is, but goes to the written X and Y.
    fn worst_error(&self, start: [f64; 2], spread: [f64; 2]) -> f64 {
        let Ok((centre, end)) = self.from(start) else { return f64::INFINITY };
        let steps = [-1.0, -0.5, 0.0, 0.5, 1.0];
        let deltas = steps.iter().flat_map(|u| steps.iter().map(move |v| [u * spread[0], v * spread[1]]));
        let apart = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]);
        deltas
            .map(|delta| {
                let actual = [start[0] + delta[0], start[1] + delta[1]];
                let Ok((meant_centre, meant_end)) = self.from(actual) else { return f64::INFINITY };
                let cut_centre = [centre[0] + delta[0], centre[1] + delta[1]];
                let cut_end = [0, 1].map(|axis| if self.end[axis].is_some() { end[axis] } else { actual[axis] });
                apart(cut_centre, meant_centre).max(apart(cut_end, meant_end))
            })
            .fold(0.0, f64::max)
    }
}

/// The modal state of the controller's interpreter (`interpretCommandString`), as the lines written to it leave it.
#[derive(Debug, Clone, PartialEq)]
pub struct Interpreter {
    units: Option<Units>,
    positioning: Positioning,
    /// The file's, which only WebControl keeps: G90.1 and G91.1 never reach the firmware.
    arc_centres: ArcCentres,
    plane: u32,
    /// `sys.lastGCommand`, which a line without a G word repeats; zero after a boot.
    motion: u32,
    xy_mm: Option<[f64; 2]>,
    /// How far from `xy_mm` the sled may be on each axis, once a status report placed it.
    spread_mm: [f64; 2],
}

impl Default for Interpreter {
    fn default() -> Interpreter {
        Interpreter::new()
    }
}

impl Interpreter {
    pub fn new() -> Interpreter {
        Interpreter {
            units: None,
            positioning: Positioning::Absolute,
            arc_centres: ArcCentres::Offsets,
            plane: 17,
            motion: 0,
            xy_mm: None,
            spread_mm: [0.0; 2],
        }
    }

    pub fn units(&self) -> Option<Units> {
        self.units
    }

    pub fn xy_mm(&self) -> Option<[f64; 2]> {
        self.xy_mm
    }

    pub fn arc_centres(&self) -> ArcCentres {
        self.arc_centres
    }

    pub fn set_arc_centres(&mut self, arc_centres: ArcCentres) {
        self.arc_centres = arc_centres;
    }

    /// Places the sled, exactly or, from a status report, to within `spread_mm` on each axis.
    pub fn set_xy_mm(&mut self, xy_mm: Option<[f64; 2]>, spread_mm: f64) {
        self.xy_mm = xy_mm;
        self.spread_mm = [spread_mm; 2];
    }

    /// Follows a written line; true when it may move the sled or changes units.
    pub fn written(&mut self, line: &str) -> bool {
        self.walk(line, None).is_ok_and(|(_, moved)| moved)
    }

    /// Follows the G90.1 and G91.1 of a line as the file had it, since what was written leaves them out.
    pub fn follow_arc_centres(&mut self, line: &str) {
        if let Some(centres) = g_numbers(line).into_iter().filter_map(ArcCentres::set_by).next_back() {
            self.arc_centres = centres;
        }
    }

    /// `line` as the firmware must get it: without G90.1 and G91.1, each arc's I and J as offsets from its start
    /// and each G91 arc's X and Y absolute. `home` is the shift the file's X and Y get, which G90.1 centres need too.
    pub fn convert(&self, line: &str, home: [f64; 2]) -> Result<String, ArcProblem> {
        self.clone().walk(line, Some(home)).map(|(line, _)| line)
    }

    fn walk(&mut self, line: &str, convert: Option<[f64; 2]>) -> Result<(String, bool), ArcProblem> {
        let upper = line.to_ascii_uppercase();
        let command = upper.trim();
        if command.starts_with('B') || command.starts_with('$') && command != "$$" {
            self.set_xy_mm(None, 0.0);
            return Ok((line.to_string(), true));
        }
        let mut edits: Vec<Edit> = Vec::new();
        let mut moved = false;
        segments(&upper).into_iter().filter(|range| !upper[range.clone()].starts_with('M')).try_for_each(|range| {
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

    /// `executeGcodeLine` for one segment: the edits that rewrite it, and whether it may move the sled.
    fn segment(&mut self, segment: &str, convert: Option<[f64; 2]>) -> Result<(Vec<Edit>, bool), ArcProblem> {
        let number = firmware_value(segment, 'G');
        if let Some(centres) = number.and_then(ArcCentres::set_by) {
            self.arc_centres = centres;
            // The firmware would switch G0 and G1 with it and ignore the rest of the segment, so all of it goes.
            return Ok((vec![(0..segment.len(), String::new())], false));
        }
        let code = number.map_or(self.motion, |code| code as u32);
        let mut moved = false;
        let mut edits = Vec::new();
        match code {
            0..=3 => {
                let arc = code >= 2;
                let has_centre = segment.contains(['I', 'J']);
                let radius = firmware_value(segment, 'R').filter(|_| arc && !has_centre);
                let absolute_centre = arc && has_centre && self.arc_centres == ArcCentres::Absolute;
                let relative_moves = self.positioning == Positioning::Relative;
                let rewrite = convert.is_some() && (radius.is_some() || absolute_centre || arc && relative_moves);
                // The firmware's G2 and G3 ignore G91, so a rewritten relative arc is written absolute.
                let relative = if arc { rewrite && relative_moves } else { relative_moves };
                let end = self.end_of(segment, relative);
                if let Some(home) = convert.filter(|_| rewrite) {
                    edits = self.arc_edits(segment, code == 2, radius, absolute_centre, home)?;
                }
                let spread = self.spread_mm;
                let placed = |letter: char| !relative && segment.contains(letter);
                self.spread_mm =
                    [('X', 0), ('Y', 1)].map(|(letter, axis)| if placed(letter) { 0.0 } else { spread[axis] });
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
            90 => self.positioning = Positioning::Absolute,
            91 => self.positioning = Positioning::Relative,
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

    /// An R or an absolute centre given as I and J offsets, and under G91 the X and Y given as where the arc ends.
    fn arc_edits(
        &self,
        segment: &str,
        clockwise: bool,
        radius: Option<f64>,
        absolute_centre: bool,
        home: [f64; 2],
    ) -> Result<Vec<Edit>, ArcProblem> {
        let relative = self.positioning == Positioning::Relative;
        if radius.is_some() && self.plane != 17 {
            return Err(ArcProblem::Plane);
        }
        if relative && segment.contains('Z') {
            return Err(ArcProblem::RelativeZ);
        }
        let needs_start = radius.is_some() || absolute_centre || relative && segment.contains(['X', 'Y']);
        let (Some(units), Some(start)) = (self.units, self.xy_mm) else {
            return if needs_start { Err(ArcProblem::StartUnknown) } else { Ok(Vec::new()) };
        };
        let scale = mm_per(units);
        let mm = |letter: char| firmware_value(segment, letter).map(|value| value * scale);
        let centre_words = [mm('I').unwrap_or(0.0), mm('J').unwrap_or(0.0)];
        let centre = match radius {
            Some(radius) => Centre::Radius(radius * scale),
            // An absolute centre is in the file's coordinates, so it moves with home as X and Y do.
            None if absolute_centre => Centre::Absolute([0, 1].map(|axis| centre_words[axis] + home[axis] * scale)),
            None => Centre::Offset(centre_words),
        };
        let geometry = Geometry { clockwise, relative, end: [mm('X'), mm('Y')], centre };
        let (centre, end) = geometry.from(start)?;
        let uncertain = self.spread_mm != [0.0; 2]
            && needs_start
            && geometry.worst_error(start, self.spread_mm) > ARC_START_TOLERANCE_MM;
        if uncertain {
            return Err(ArcProblem::StartUncertain);
        }
        let offset = |axis: usize| four((centre[axis] - start[axis]) / scale);
        let offsets = format!("I{} J{}", offset(0), offset(1));
        let mut edits = Vec::new();
        if radius.is_some() {
            edits.extend(word_range(segment, 'R').map(|range| (range, offsets)));
        } else if absolute_centre {
            match (word_range(segment, 'I'), word_range(segment, 'J')) {
                (Some(i), Some(j)) => edits.extend([(i, format!("I{}", offset(0))), (j, format!("J{}", offset(1)))]),
                (i, j) => edits.extend(i.or(j).map(|range| (range, offsets))),
            }
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

    const NO_HOME: [f64; 2] = [0.0, 0.0];

    fn close(a: [f64; 2], b: [f64; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9
    }

    fn after(lines: &[&str]) -> Interpreter {
        let mut interpreter = Interpreter::new();
        lines.iter().for_each(|line| {
            interpreter.follow_arc_centres(line);
            interpreter.written(line);
        });
        interpreter
    }

    fn converted(at: &Interpreter, line: &str) -> Result<String, ArcProblem> {
        at.convert(line, NO_HOME)
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
        assert_eq!(converted(&at, "G2 X20 Y0 R10 F300 ").unwrap(), "G2 X20 Y0 I0.0000 J-10.0000 F300 ");
        assert_eq!(converted(&at, "G03 X20 Y0 R10").unwrap(), "G03 X20 Y0 I10.0000 J0.0000");
        let long = after(&["G21 ", "G0 X20 Y0 "]);
        assert_eq!(converted(&long, "G3 X10 Y-10 R-10 ").unwrap(), "G3 X10 Y-10 I-10.0000 J0.0000 ");
    }

    #[test]
    fn inches_are_converted_in_inches() {
        let mut at = after(&["G20 ", "G0 X1 Y1 "]);
        assert_eq!(converted(&at, "G2 X1.5 Y0.5 R0.5 ").unwrap(), "G2 X1.5 Y0.5 I0.0000 J-0.5000 ");
        assert!(at.written("G2 X1.5 Y0.5 I0 J-0.5 "));
        assert!(close(at.xy_mm().unwrap(), [38.1, 12.7]));
    }

    #[test]
    fn a_relative_r_arc_is_written_absolute_because_the_firmware_reads_arcs_so() {
        let at = after(&["G21 ", "G0 X10 Y10 ", "G91 "]);
        assert_eq!(converted(&at, "G2 X10 Y-10 R10 ").unwrap(), "G2 X20.0000 Y0.0000 I0.0000 J-10.0000 ");
        assert_eq!(converted(&at, "G3 Y-20 R-10 ").unwrap(), "G3 Y-10.0000 I0.0000 J-10.0000 ");
        let mut relative = at.clone();
        relative.written("G1 X1 Y1 ");
        assert!(close(relative.xy_mm().unwrap(), [11.0, 11.0]));
        relative.written("G2 X0 Y0 I-5 J-5 ");
        assert!(close(relative.xy_mm().unwrap(), [0.0, 0.0]));
    }

    #[test]
    fn a_relative_i_j_arc_is_written_absolute_and_one_with_z_is_refused() {
        let at = after(&["G21 ", "G0 X10 Y10 ", "G91 "]);
        assert_eq!(converted(&at, "G2 X10 Y-10 I0 J-10 F300 ").unwrap(), "G2 X20.0000 Y0.0000 I0 J-10 F300 ");
        assert_eq!(converted(&at, "g3 y5 i0 j2.5").unwrap(), "g3 Y15.0000 i0 j2.5");
        assert_eq!(converted(&at, "G2 I5 J0 ").unwrap(), "G2 I5 J0 ");
        assert_eq!(converted(&at, "G1 X5 ").unwrap(), "G1 X5 ");
        assert_eq!(converted(&at, "G2 X1 Y1 I1 J0 Z-1 ").unwrap_err(), ArcProblem::RelativeZ);
        assert_eq!(converted(&after(&["G21 ", "G91 "]), "G2 X1 I1 J0 ").unwrap_err(), ArcProblem::StartUnknown);
        assert_eq!(converted(&after(&["G21 ", "G91 "]), "G2 I1 J0 ").unwrap(), "G2 I1 J0 ");
        let mut absolute = at.clone();
        absolute.written("G90 ");
        assert_eq!(converted(&absolute, "G2 X10 Y-10 I0 J-10 Z-1 ").unwrap(), "G2 X10 Y-10 I0 J-10 Z-1 ");
    }

    #[test]
    fn g91_1_and_g90_1_never_reach_the_firmware_nor_change_g0_and_g1() {
        let at = after(&["G21 ", "G0 X10 Y10 "]);
        assert_eq!(converted(&at, "G90 G91.1 ").unwrap(), "G90 ");
        assert_eq!(converted(&at, "G71G91.1").unwrap(), "G71");
        assert_eq!(converted(&at, "G91.1 X1 F100 ").unwrap(), "");
        assert_eq!(converted(&at, "g90.1").unwrap(), "");
        assert_eq!(converted(&at, "G0 X5 G091.10 Y2 ").unwrap(), "G0 X5 ");
        let mut moved = after(&["G21 ", "G0 X10 Y10 ", "G91.1 "]);
        assert!(!moved.written("G91.1 X5 "));
        assert!(moved.written("G1 X1 "));
        assert!(close(moved.xy_mm().unwrap(), [1.0, 10.0]));
        assert_eq!(converted(&moved, "G2 X20 Y0 I0 J-10 Z-1 ").unwrap(), "G2 X20 Y0 I0 J-10 Z-1 ");
    }

    #[test]
    fn under_g90_1_each_arcs_centre_is_written_as_its_offset_from_the_start() {
        let at = after(&["G21 ", "G0 X10 Y10 ", "G90.1 "]);
        assert_eq!(at.arc_centres(), ArcCentres::Absolute);
        assert_eq!(converted(&at, "G2 X20 Y0 I10 J0 F300 ").unwrap(), "G2 X20 Y0 I0.0000 J-10.0000 F300 ");
        assert_eq!(converted(&at, "G3 X20 Y0 I20 ").unwrap(), "G3 X20 Y0 I10.0000 J-10.0000 ");
        assert_eq!(converted(&at, "G3 X20 Y0 J10 ").unwrap(), "G3 X20 Y0 I-10.0000 J0.0000 ");
        assert_eq!(converted(&at, "G2 X20 Y0 R10 ").unwrap(), "G2 X20 Y0 I0.0000 J-10.0000 ");
        assert_eq!(converted(&at, "G2 X20 Y0 ").unwrap(), "G2 X20 Y0 ");
        assert_eq!(converted(&at, "G1 X5 I3 ").unwrap(), "G1 X5 I3 ");
        assert_eq!(converted(&at, "G91.1 G2 X20 Y0 I0 J-10 ").unwrap(), "G2 X20 Y0 I0 J-10 ");
        assert_eq!(converted(&after(&["G21 ", "G90.1 "]), "G2 I1 J0 ").unwrap_err(), ArcProblem::StartUnknown);
        let offsets = after(&["G21 ", "G0 X10 Y10 ", "G90.1 ", "G91.1 "]);
        assert_eq!(converted(&offsets, "G2 X20 Y0 I0 J-10 ").unwrap(), "G2 X20 Y0 I0 J-10 ");
        let same_line = after(&["G21 ", "G0 X10 Y10 "]);
        assert_eq!(converted(&same_line, "G90.1 G2 X20 Y0 I10 J0 ").unwrap(), "G2 X20 Y0 I0.0000 J-10.0000 ");
    }

    #[test]
    fn a_g90_1_centre_is_shifted_to_home_like_x_and_y_and_kept_in_the_files_units() {
        let at = after(&["G21 ", "G0 X110 Y60 ", "G90.1 "]);
        let home = [100.0, 50.0];
        assert_eq!(at.convert("G2 X120 Y50 I10 J0 ", home).unwrap(), "G2 X120 Y50 I0.0000 J-10.0000 ");
        let inches = after(&["G20 ", "G0 X2 Y2 ", "G90.1 "]);
        assert_eq!(inches.convert("G2 X3 Y1 I1 J0 ", [1.0, 1.0]).unwrap(), "G2 X3 Y1 I0.0000 J-1.0000 ");
    }

    #[test]
    fn a_g90_1_arc_under_g91_has_absolute_x_and_y_and_an_offset_centre() {
        let at = after(&["G21 ", "G0 X10 Y10 ", "G91 ", "G90.1 "]);
        assert_eq!(converted(&at, "G2 X10 Y-10 I10 J0 ").unwrap(), "G2 X20.0000 Y0.0000 I0.0000 J-10.0000 ");
        assert_eq!(at.convert("G2 X10 Y-10 I0 J-10 ", [10.0, 10.0]).unwrap(), "G2 X20.0000 Y0.0000 I0.0000 J-10.0000 ");
        assert_eq!(converted(&at, "G2 X1 Y1 I1 J0 Z-1 ").unwrap_err(), ArcProblem::RelativeZ);
    }

    #[test]
    fn the_arc_centre_mode_follows_the_last_word_and_only_g90_1_or_g91_1_set_it() {
        let mut at = Interpreter::default();
        at.follow_arc_centres("G90 G90.1 G1 X1 ");
        assert_eq!(at.arc_centres(), ArcCentres::Absolute);
        at.follow_arc_centres("G91 G90 G1 X1 ");
        assert_eq!(at.arc_centres(), ArcCentres::Absolute);
        at.follow_arc_centres("G90.1 g91.1 ");
        assert_eq!(at.arc_centres(), ArcCentres::Offsets);
        at.set_arc_centres(ArcCentres::Absolute);
        assert_eq!(at.arc_centres(), ArcCentres::Absolute);
        assert_eq!(ArcCentres::default().word(), "G91.1 ");
        assert_eq!(ArcCentres::Absolute.word(), "G90.1 ");
        assert_eq!(ArcCentres::set_by(91.0), None);
    }

    #[test]
    fn a_file_with_arc_centre_modes_gets_a_note_naming_them() {
        let lines = |text: &[&str]| text.iter().map(|line| line.to_string()).collect::<Vec<_>>();
        assert_eq!(
            arc_centre_note(&lines(&["G90 g91.1 ", "G1 X1 "])).unwrap(),
            "This file uses G91.1; WebControl converts it for the Maslow firmware."
        );
        assert!(arc_centre_note(&lines(&["G90.1 "])).unwrap().contains("uses G90.1;"));
        assert!(arc_centre_note(&lines(&["G91.1 ", "G90.1 "])).unwrap().contains("uses G91.1 and G90.1;"));
        assert_eq!(arc_centre_note(&lines(&["G91 ", "G90 ", "(G90.1) "])), None);
    }

    fn reported(lines: &[&str], xy_mm: [f64; 2], spread_mm: f64) -> Interpreter {
        let mut at = after(lines);
        at.set_xy_mm(Some(xy_mm), spread_mm);
        at
    }

    #[test]
    fn a_near_half_circle_from_a_reported_start_is_refused_and_from_a_commanded_one_converted() {
        let inches = reported(&["G20 "], [0.0, 0.0], 0.127);
        assert_eq!(converted(&inches, "G2 X1 Y0 R0.5 ").unwrap_err(), ArcProblem::StartUncertain);
        let mm = reported(&["G21 "], [0.0, 0.0], 0.005);
        assert_eq!(converted(&mm, "G2 X10 Y0 R5.001 ").unwrap_err(), ArcProblem::StartUncertain);
        let commanded = after(&["G20 ", "G0 X0 Y0 "]);
        assert_eq!(converted(&commanded, "G2 X1 Y0 R0.5 ").unwrap(), "G2 X1 Y0 I0.5000 J0.0000 ");
        let mut placed = inches.clone();
        placed.written("G0 X0 Y0 ");
        assert_eq!(converted(&placed, "G2 X1 Y0 R0.5 ").unwrap(), "G2 X1 Y0 I0.5000 J0.0000 ");
    }

    #[test]
    fn a_low_sensitivity_arc_from_a_reported_start_is_converted() {
        let mm = reported(&["G21 "], [10.0, 10.0], 0.005);
        assert_eq!(converted(&mm, "G2 X20 Y0 R10 ").unwrap(), "G2 X20 Y0 I0.0000 J-10.0000 ");
        let absolute = reported(&["G21 ", "G90.1 "], [10.0, 10.0], 0.005);
        assert_eq!(converted(&absolute, "G2 X20 Y0 I10 J0 ").unwrap(), "G2 X20 Y0 I0.0000 J-10.0000 ");
        let inches = reported(&["G20 ", "G90.1 "], [25.4, 25.4], 0.127);
        assert_eq!(converted(&inches, "G2 X2 Y0 I1 J0 ").unwrap_err(), ArcProblem::StartUncertain);
        let offsets = reported(&["G20 "], [25.4, 25.4], 0.127);
        assert_eq!(converted(&offsets, "G2 X2 Y0 I0 J-1 G90.1 ").unwrap(), "G2 X2 Y0 I0 J-1 ");
    }

    #[test]
    fn a_relative_move_keeps_a_reported_starts_uncertainty_and_an_absolute_one_clears_it_per_axis() {
        let mut at = reported(&["G20 ", "G91 "], [0.0, 0.0], 0.127);
        at.written("G1 X1 Y1 ");
        assert_eq!(converted(&at, "G2 X1 Y-1 I1 J0 ").unwrap_err(), ArcProblem::StartUncertain);
        at.written("G90 G0 X1 ");
        at.written("G91 ");
        assert_eq!(converted(&at, "G2 X0 Y1 I1 J0 ").unwrap_err(), ArcProblem::StartUncertain);
        at.written("G90 G0 Y1 ");
        at.written("G91 ");
        assert_eq!(converted(&at, "G2 X1 Y-1 I1 J0 ").unwrap(), "G2 X2.0000 Y0.0000 I1 J0 ");
    }

    #[test]
    fn the_worst_error_counts_a_start_whose_arc_has_no_centre() {
        let tight =
            Geometry { clockwise: true, relative: false, end: [Some(10.0), Some(0.0)], centre: Centre::Radius(5.0) };
        assert_eq!(tight.worst_error([0.0, 0.0], [1.0, 1.0]), f64::INFINITY);
        assert_eq!(tight.worst_error([-2.0, 0.0], [0.0, 0.0]), f64::INFINITY);
        assert_eq!(tight.worst_error([0.0, 0.0], [0.0, 0.0]), 0.0);
    }

    #[test]
    fn lower_case_words_and_modal_arcs_are_converted() {
        let at = after(&["g21", "g0 x10 y10"]);
        assert_eq!(converted(&at, "g2 x20 y0 r10").unwrap(), "g2 x20 y0 I0.0000 J-10.0000");
        let modal = after(&["G21 ", "G0 X10 Y10 ", "G2 X20 Y0 I0 J-10 "]);
        assert_eq!(converted(&modal, "X10 Y-10 R10 ").unwrap(), "X10 Y-10 I-10.0000 J0.0000 ");
        let linear = after(&["G21 ", "G0 X10 Y10 ", "G1 X20 "]);
        assert_eq!(converted(&linear, "X30 R10 ").unwrap(), "X30 R10 ");
    }

    #[test]
    fn lower_case_units_and_g91_written_before_an_arc_are_followed() {
        let at = after(&["g20", "g0 x1 y1", "g91"]);
        assert_eq!(converted(&at, "g2 x1 y-1 r1").unwrap(), "g2 X2.0000 Y0.0000 I0.0000 J-1.0000");
        assert_eq!(converted(&at, "g3 x1 y1 i0 j1 z-1").unwrap_err(), ArcProblem::RelativeZ);
    }

    #[test]
    fn each_command_on_a_line_is_read_in_turn() {
        let at = after(&["G21 "]);
        assert_eq!(
            converted(&at, "G0 X10 Y10 G2 X20 Y0 R10 M3 G3 X30 Y10 R10 ").unwrap(),
            "G0 X10 Y10 G2 X20 Y0 I0.0000 J-10.0000 M3 G3 X30 Y10 I0.0000 J10.0000 "
        );
        assert_eq!(converted(&after(&["G0 X1 Y1 "]), "G20 G2 X2 Y2 R1 ").unwrap_err(), ArcProblem::StartUnknown);
        assert_eq!(
            converted(&after(&["G20 ", "G0 X1 Y1 "]), "T2 G2 X2 Y0 R1 ").unwrap(),
            "T2 G2 X2 Y0 I0.0000 J-1.0000 "
        );
    }

    #[test]
    fn lines_without_an_r_arc_are_left_alone() {
        let at = after(&["G21 ", "G0 X0 Y0 "]);
        ["G2 X10 Y0 I5 J0 R3 ", "G1 X10 R3 ", "B14 R1 ", "$13=R ", "M3 R1 ", "G4 P1 "]
            .iter()
            .for_each(|line| assert_eq!(converted(&at, line).unwrap(), *line));
    }

    #[test]
    fn r_arcs_the_controller_cannot_cut_are_refused() {
        let at = after(&["G21 ", "G0 X10 Y10 "]);
        let refused = |at: &Interpreter, line| converted(at, line).unwrap_err();
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
        at.set_xy_mm(Some([1.0, 2.0]), 0.0);
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
            ArcProblem::StartUncertain,
        ];
        all.iter().for_each(|problem| {
            assert!(!problem.title().is_empty() && problem.reason().starts_with("is "), "{problem:?}");
        });
        assert!(ArcProblem::StartUnknown.advice().starts_with("Resume returns"));
        assert_eq!(ArcProblem::StartUncertain.advice(), ArcProblem::StartUnknown.advice());
        assert!(ArcProblem::Plane.advice().starts_with("Resume stops here again"));
        assert_eq!(Interpreter::default(), Interpreter::new());
    }
}
