//! The 3D preview (`GCodeFile.updateGcode` and its draw methods): the moves `gcodeUpdateCompressed` carries, in
//! inches, with circles marking plunges, retracts, spindle and tool changes.

use std::f64::consts::PI;
use std::io::Write;

use flate2::Compression;
use flate2::write::GzEncoder;
use serde::Serialize;
use wc_protocol::main::Units;

use crate::load::unreadable_word;
use crate::{py, words};

/// Python renders at most this many lines; the rest still cut.
pub const MAX_LINES: usize = 300_000;

const MILLIMETERS: f64 = 1.0 / 25.4;
const VALID_PREFIXES: [&str; 9] = ["G00", "G0 ", "G1 ", "G01", "G2 ", "G02", "G3 ", "G03", "G17"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MoveKind {
    Line,
    Circle,
}

/// One entry of the `gcodeMoves` payload. A circle's second point is (size, argument, z).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Move {
    #[serde(rename = "type")]
    pub kind: MoveKind,
    pub points: Vec<[f64; 3]>,
    pub dashed: bool,
    pub command: Option<&'static str>,
}

impl Move {
    fn new(kind: MoveKind, dashed: bool, command: Option<&'static str>, start: [f64; 3]) -> Move {
        Move { kind, points: vec![start], dashed, command }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Preview {
    pub moves: Vec<Move>,
    /// `gcodeFileUnits` once the file is drawn: the machine's units until the file's last G20/G21.
    pub file_units: Units,
    /// The console lines Python logs while drawing.
    pub warnings: Vec<String>,
    /// The alert Python raises when only the first `MAX_LINES` lines are drawn.
    pub too_large: Option<String>,
}

/// Draws `lines` as loaded, starting from the origin with `machine` units as the file's.
pub fn render(lines: &[String], machine: Units) -> Preview {
    let mut drawing = Drawing {
        position: [0.0; 3],
        scale: if machine == Units::Mm { MILLIMETERS } else { 1.0 },
        relative: false,
        prepend: "G00 ".to_string(),
        tool: 0,
        file_units: machine,
        moves: Vec::new(),
        warnings: Vec::new(),
    };
    lines.iter().take(MAX_LINES).for_each(|line| drawing.line(line));
    let too_large = (lines.len() > MAX_LINES).then(|| too_large_alert(lines.len()));
    Preview { moves: drawing.moves, file_units: drawing.file_units, warnings: drawing.warnings, too_large }
}

pub fn too_large_alert(count: usize) -> String {
    format!(
        "The current file contains {count} lines of gcode.  Rendering all {count} lines simultaneously may crash the \
         program, therefore, only the first {MAX_LINES}lines are shown here.  The complete program will cut if you \
         choose to do so unless the home position is moved from (0,0)."
    )
}

/// The gzipped JSON `gcodeUpdateCompressed` sends.
pub fn compress(moves: &[Move]) -> Vec<u8> {
    gzip(&serde_json::to_vec(moves).expect("moves serialize"))
}

pub(crate) fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(bytes).expect("writing to memory");
    encoder.finish().expect("writing to memory")
}

struct Drawing {
    position: [f64; 3],
    scale: f64,
    relative: bool,
    prepend: String,
    tool: i64,
    file_units: Units,
    moves: Vec<Move>,
    warnings: Vec<String>,
}

/// Python's `text[start:end]` by character, clamped like a slice.
fn slice(text: &str, start: usize, end: usize) -> String {
    text.chars().skip(start).take(end.saturating_sub(start)).collect()
}

/// Python's `text[text.find(c):]`, which is the last character when `c` is missing.
fn from_char(text: &str, c: char) -> String {
    match text.find(c) {
        Some(at) => text[at..].to_string(),
        None => text.chars().last().map(String::from).unwrap_or_default(),
    }
}

impl Drawing {
    fn line(&mut self, raw: &str) {
        let text = words::replace_semicolons(&words::replace_parens(raw, "\n"), "\n");
        let parts: Vec<&str> = text.split('G').collect();
        if parts.len() > 1 {
            parts.iter().filter(|part| !part.is_empty()).for_each(|part| self.command(&format!("G{part}")));
        } else {
            self.command(&text);
        }
    }

    fn command(&mut self, text: &str) {
        let mut full = format!("{} ", py::upper(text));
        let mut g_word = match full.find('G') {
            Some(at) => {
                let start = full[..at].chars().count();
                slice(&full, start, start + 3)
            }
            None => String::new(),
        };
        if VALID_PREFIXES.contains(&g_word.as_str()) {
            self.prepend = g_word.clone();
        }
        if !full.contains('G') {
            full = format!("{} {full}", self.prepend);
            g_word = self.prepend.clone();
        }
        match g_word.as_str() {
            "G00" | "G0 " => self.draw_line(&full, true),
            "G01" | "G1 " => self.draw_line(&full, false),
            "G02" | "G2 " => self.draw_arc(&full, true),
            "G03" | "G3 " => self.draw_arc(&full, false),
            "G18" => self.warnings.push("G18 not supported".into()),
            "G20" => {
                self.scale = 1.0;
                self.file_units = Units::Inches;
            }
            "G21" => {
                self.scale = MILLIMETERS;
                self.file_units = Units::Mm;
            }
            "G90" => self.relative = false,
            "G91" => self.relative = true,
            _ => {}
        }
        let t_word = from_char(&full, 'T');
        if t_word.starts_with('T') {
            self.tool = tool_number(&t_word);
        }
        let m_word = from_char(&full, 'M');
        if m_word.starts_with('M') {
            self.draw_m_command(&m_word);
        }
    }

    /// `_wordValue`: the scaled number, or `None` (logging it) when the word cannot be read.
    fn value(&mut self, line: &str, letter: char) -> Option<f64> {
        words::value(line, letter).unwrap_or_else(|()| {
            self.warnings.push(unreadable_word(letter, line));
            None
        })
    }

    fn draw_line(&mut self, line: &str, rapid: bool) {
        let [x0, y0, z0] = self.position;
        let mut target = self.position;
        let is_relative = self.relative;
        let relative = |origin: f64, value: f64| if is_relative { origin + value } else { value };
        if let Some(x) = self.value(line, 'X') {
            target[0] = relative(x0, x * self.scale);
        }
        if let Some(y) = self.value(line, 'Y') {
            target[1] = relative(y0, y * self.scale);
        }
        if let Some(z) = self.value(line, 'Z') {
            target[2] = z * self.scale;
        }
        if (x0 - target[0]).abs() > 0.0001 || (y0 - target[1]).abs() > 0.0001 {
            if rapid {
                let mut dashed = Move::new(MoveKind::Line, true, None, self.position);
                dashed.points.push(target);
                self.moves.push(dashed);
            } else {
                self.continue_line();
                self.moves.last_mut().expect("a line").points.push(target);
            }
        }
        if (target[2] - z0).abs() >= 0.05 {
            let radius = if target[2] - z0 > 0.0 { 2.0 } else { 1.0 };
            let mut circle = Move::new(MoveKind::Circle, false, None, self.position);
            circle.points.push([radius, 0.0, target[2]]);
            self.moves.push(circle);
        }
        self.position = target;
    }

    /// Starts a new solid line at the current position unless the last move is one.
    fn continue_line(&mut self) {
        if !self.moves.last().is_some_and(|last| !last.dashed && last.kind == MoveKind::Line) {
            self.moves.push(Move::new(MoveKind::Line, false, None, self.position));
        }
    }

    fn draw_arc(&mut self, line: &str, clockwise: bool) {
        let [x0, y0, z0] = self.position;
        let mut target = self.position;
        let mut centre_offset = [0.0, 0.0];
        if let Some(x) = self.value(line, 'X') {
            target[0] = x * self.scale;
        }
        if let Some(y) = self.value(line, 'Y') {
            target[1] = y * self.scale;
        }
        if let Some(i) = self.value(line, 'I') {
            centre_offset[0] = i * self.scale;
        }
        if let Some(j) = self.value(line, 'J') {
            centre_offset[1] = j * self.scale;
        }
        if let Some(z) = self.value(line, 'Z') {
            target[2] = z * self.scale;
        }
        let radius = (centre_offset[0].powi(2) + centre_offset[1].powi(2)).sqrt();
        let (centre_x, centre_y) = (x0 + centre_offset[0], y0 + centre_offset[1]);
        let positive = |angle: f64| if angle < 0.0 { angle + 2.0 * PI } else { angle };
        let mut start = positive((y0 - centre_y).atan2(x0 - centre_x));
        let mut end = positive((target[1] - centre_y).atan2(target[0] - centre_x));
        let direction = if clockwise {
            if start < end {
                start += 2.0 * PI;
            }
            -1.0
        } else {
            if end < start {
                end += 2.0 * PI;
            }
            1.0
        };
        let sweep = if start == end { 6.28313530718 } else { (start - end).abs() };
        self.continue_line();
        let z_step = (target[2] - z0) / (sweep / (0.1 * direction)).abs();
        let mut angle = 0.0_f64;
        let mut step = 0.0;
        let line = self.moves.last_mut().expect("a line");
        while angle.abs() < sweep {
            line.points.push([
                centre_x + radius * (start + angle).cos(),
                centre_y + radius * (start + angle).sin(),
                z0 + z_step * step,
            ]);
            angle += 0.1 * direction;
            step += 1.0;
        }
        line.points.push(target);
        self.position = target;
    }

    fn draw_m_command(&mut self, m_word: &str) {
        let (size, argument, command) = match m_command(m_word) {
            3 | 4 if spindle_speed(m_word) == 0.0 => (5.0, 0.0, Some("SpindleOff")),
            3 => (3.0, spindle_speed(m_word), Some("SpindleOnCW")),
            4 => (4.0, spindle_speed(m_word), Some("SpindleOnCCW")),
            5 => (5.0, 0.0, Some("SpindleOff")),
            6 => (6.0, self.tool as f64, Some("ToolChange")),
            _ => (0.0, 0.0, None),
        };
        let mut circle = Move::new(MoveKind::Circle, false, command, self.position);
        circle.points.push([size, argument, self.position[2]]);
        self.moves.push(circle);
    }
}

/// `getMCommand`: the number between the M and the next space, or 0.
fn m_command(m_word: &str) -> i64 {
    let end = m_word.find(' ').unwrap_or(m_word.len());
    py::int(&m_word[1..end]).unwrap_or(0)
}

/// `getSpindleSpeed`: everything after the first S read as a float, the whole word without an S, -1 if unreadable.
fn spindle_speed(m_word: &str) -> f64 {
    let speed = m_word.find('S').map_or(m_word, |at| &m_word[at + 1..]);
    if speed.is_empty() { 0.0 } else { py::float(speed).unwrap_or(-1.0) }
}

/// `getToolNumber`: the text between the T and the first M (or the last character), as an int; -1 if unreadable.
fn tool_number(t_word: &str) -> i64 {
    let count = t_word.chars().count();
    let end = t_word.find('M').map_or(count.saturating_sub(1), |at| t_word[..at].chars().count());
    let number = slice(t_word, 1, end);
    if number.is_empty() { 0 } else { py::int(&number).unwrap_or(-1) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load::filter;

    fn draw(text: &str, units: Units) -> Preview {
        render(&filter(text, None).unwrap(), units)
    }

    fn last_point(preview: &Preview) -> [f64; 3] {
        *preview.moves.last().unwrap().points.last().unwrap()
    }

    #[test]
    fn a_metric_file_renders_in_inches() {
        let preview = draw("G21\nG1 X25.4 Y50.8\n", Units::Inches);
        let [x, y, z] = last_point(&preview);
        assert!((x - 1.0).abs() < 1e-12 && (y - 2.0).abs() < 1e-12 && z == 0.0);
        assert_eq!(preview.file_units, Units::Mm);
    }

    #[test]
    fn a_file_without_units_is_taken_to_be_in_machine_units() {
        assert_eq!(draw("G0 X1 Y1\n", Units::Inches).file_units, Units::Inches);
        assert_eq!(draw("G20\nG0 X1", Units::Mm).file_units, Units::Inches);
    }

    #[test]
    fn the_preview_skips_a_word_it_cannot_read_and_draws_the_rest() {
        let preview = draw("G0 X1\nG1 X2 Z`3\nG1 X3 Y4\n", Units::Inches);
        assert_eq!(last_point(&preview)[..2], [3.0, 4.0]);
        assert_eq!(preview.warnings, ["Preview skipped an unreadable Z word: G1 X2 Z`3"]);
    }

    #[test]
    fn rapids_are_dashed_feeds_are_joined_and_z_changes_are_circled() {
        let preview = draw("G0 X1\nG1 X2\nG1 X3\nG1 Z-1\nG0 Z1", Units::Inches);
        let kinds: Vec<_> = preview.moves.iter().map(|m| (m.kind, m.dashed, m.points.len())).collect();
        assert_eq!(
            kinds,
            [
                (MoveKind::Line, true, 2),
                (MoveKind::Line, false, 3),
                (MoveKind::Circle, false, 2),
                (MoveKind::Circle, false, 2)
            ]
        );
        assert_eq!(preview.moves[2].points[1], [1.0, 0.0, -1.0]);
        assert_eq!(preview.moves[3].points[1], [2.0, 0.0, 1.0]);
    }

    #[test]
    fn relative_moves_add_to_the_position_but_z_does_not() {
        let preview = draw("G0 X1 Y1\nG91\nG1 X1 Y1 Z-1\nG90\nG1 X0", Units::Inches);
        assert_eq!(preview.moves[1].points, [[1.0, 1.0, 0.0], [2.0, 2.0, -1.0]]);
        assert_eq!(preview.moves[3].points, [[2.0, 2.0, -1.0], [0.0, 2.0, -1.0]]);
    }

    #[test]
    fn a_line_without_a_g_word_continues_the_last_motion_once_there_is_one() {
        assert!(draw("X1 Y1", Units::Inches).moves.is_empty());
        assert_eq!(draw("G1 X1\nY1", Units::Inches).moves[0].points.len(), 3);
        assert_eq!(draw("G17\nX1 Y1", Units::Inches).moves.len(), 0);
    }

    #[test]
    fn arcs_are_drawn_in_tenths_of_a_radian_to_the_end_point() {
        let quarter = draw("G1 X1\nG2 X2 Y-1 I0 J-1", Units::Inches);
        let points = &quarter.moves[0].points;
        assert_eq!(points.len(), 2 + 16 + 1);
        assert_eq!(*points.last().unwrap(), [2.0, -1.0, 0.0]);
        let ccw = draw("G3 X0 Y0 I1 J0 Z-1", Units::Inches);
        assert_eq!(ccw.moves[0].points.len(), 1 + 63 + 1);
        assert_eq!(*ccw.moves[0].points.last().unwrap(), [0.0, 0.0, -1.0]);
        let cw_full = draw("G2 X0 Y0 I1 J0", Units::Inches);
        assert_eq!(cw_full.moves[0].points.len(), 1 + 63 + 1);
    }

    #[test]
    fn m_commands_draw_circles_with_their_meaning() {
        let preview = draw("T3\nM3 S1000\nM4\nM03 S0\nM5\nM6\nM8\nM6T2", Units::Inches);
        let circles: Vec<_> = preview.moves.iter().map(|m| (m.command, m.points[1][0], m.points[1][1])).collect();
        assert_eq!(
            circles,
            [
                (Some("SpindleOnCW"), 3.0, 1000.0),
                (Some("SpindleOnCCW"), 4.0, -1.0),
                (Some("SpindleOff"), 5.0, 0.0),
                (Some("SpindleOff"), 5.0, 0.0),
                (Some("ToolChange"), 6.0, 3.0),
                (None, 0.0, 0.0),
                (Some("ToolChange"), 6.0, 2.0),
            ]
        );
    }

    #[test]
    fn tool_numbers_read_like_python() {
        assert_eq!(tool_number("T2 "), 2);
        assert_eq!(tool_number("T2 M6 "), 2);
        assert_eq!(tool_number("T2.0 "), -1);
        assert_eq!(tool_number("TM6 "), 0);
        assert_eq!(m_command("M6T2 "), 0);
        assert_eq!(spindle_speed("M3 S"), 0.0);
    }

    #[test]
    fn g18_is_reported_and_comments_are_not_drawn() {
        let preview = draw("G18\n(G0 X5)\nG0 X1 ; G0 X9", Units::Inches);
        assert_eq!(preview.warnings, ["G18 not supported"]);
        assert_eq!(last_point(&preview), [1.0, 0.0, 0.0]);
    }

    #[test]
    fn only_the_first_max_lines_are_drawn() {
        let lines = vec!["G1 X1 ".to_string(); MAX_LINES + 1];
        let preview = render(&lines, Units::Inches);
        assert!(preview.too_large.unwrap().starts_with("The current file contains 300001 lines of gcode."));
        assert_eq!(preview.moves[0].points.len(), 2);
        assert_eq!(render(&lines[..2], Units::Inches).too_large, None);
    }

    #[test]
    fn the_payload_is_gzipped_json() {
        let moves = draw("G0 X1", Units::Inches).moves;
        let mut json = String::new();
        std::io::Read::read_to_string(&mut flate2::read::GzDecoder::new(&compress(&moves)[..]), &mut json).unwrap();
        assert_eq!(json, r#"[{"type":"line","points":[[0.0,0.0,0.0],[1.0,0.0,0.0]],"dashed":true,"command":null}]"#);
    }

    #[test]
    fn python_slices_are_by_character_and_clamp() {
        assert_eq!(slice("üG1 X", 1, 4), "G1 ");
        assert_eq!(slice("ab", 1, 9), "b");
        assert_eq!(from_char("abc", 'z'), "c");
        assert_eq!(from_char("", 'z'), "");
    }
}
