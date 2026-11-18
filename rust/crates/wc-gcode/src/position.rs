//! `Actions.findPositionAt`: the file's X, Y and Z target before a line, for `gcodePositionMessage`.

use crate::arc::ArcCentres;
use crate::{GcodeError, Positioning, py, words};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    /// The last F the file set before the line, which Python stores as `data.feedRate`.
    pub feed_rate: Option<f64>,
}

/// Parses `lines[..index]`, splitting each line at its G words so commands on one line are read in turn. Unlike the
/// restart state it ignores home and units.
pub fn find_position_at(lines: &[String], index: usize) -> Result<Position, GcodeError> {
    let before = lines.get(..index).ok_or(GcodeError::IndexOutOfRange { index, len: lines.len() })?;
    let mut position = Position { x: 0.0, y: 0.0, z: 0.0, feed_rate: None };
    let mut positioning = Positioning::Absolute;
    before.iter().enumerate().try_for_each(|(line_index, raw)| {
        let line = words::code(raw);
        if py::isspace(&line) {
            return Ok(());
        }
        split_at_g(&line)
            .iter()
            .try_for_each(|part| read(part, &mut position, &mut positioning))
            .map_err(|()| GcodeError::Unreadable { index: line_index })
    })?;
    Ok(position)
}

/// Python's `['G' + part for part in filter(None, re.split("(G)", line))]`.
fn split_at_g(line: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut rest = line;
    while let Some(at) = rest.find('G') {
        parts.extend((at > 0).then(|| format!("G{}", &rest[..at])));
        parts.push("GG".to_string());
        rest = &rest[at + 1..];
    }
    parts.extend((!rest.is_empty()).then(|| format!("G{rest}")));
    parts
}

fn read(line: &str, position: &mut Position, positioning: &mut Positioning) -> Result<(), ()> {
    let codes = words::numbers('G', line);
    // WebControl leaves a G90.1 or G91.1 command out of what it writes, axis words and all.
    if codes.iter().any(|&code| ArcCentres::set_by(code).is_some()) {
        return Ok(());
    }
    if codes.contains(&90.0) {
        *positioning = Positioning::Absolute;
    }
    if codes.contains(&91.0) {
        *positioning = Positioning::Relative;
    }
    let relative = *positioning == Positioning::Relative;
    let axes = [('X', &mut position.x), ('Y', &mut position.y), ('Z', &mut position.z)];
    axes.into_iter().filter(|(letter, _)| line.contains(*letter)).try_for_each(|(letter, axis)| {
        let value = words::value(line, letter)?.ok_or(())?;
        *axis = if relative { *axis + value } else { value };
        Ok(())
    })?;
    if line.contains('F') {
        position.feed_rate = Some(words::value(line, 'F')?.ok_or(())?);
    }
    Ok(())
}

/// A word whose number Python's `float()` would not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unreadable;

/// A word's number as Python's `float()` reads it: `Ok(None)` without the word.
pub fn word(line: &str, letter: char) -> Result<Option<f64>, Unreadable> {
    words::value(line, letter).map_err(|()| Unreadable)
}

/// `sendGCodePositionUpdate` without recalculating: the target of one streamed line, from the one before it.
pub fn line_target(previous: Position, positioning: Positioning, line: &str) -> Result<Position, Unreadable> {
    let line = &line.to_ascii_uppercase();
    let relative = positioning == Positioning::Relative;
    let axis = |letter: char, before: f64| -> Result<f64, Unreadable> {
        Ok(word(line, letter)?.map_or(before, |value| if relative { before + value } else { value }))
    };
    Ok(Position {
        x: axis('X', previous.x)?,
        y: axis('Y', previous.y)?,
        z: axis('Z', previous.z)?,
        feed_rate: word(line, 'F')?.or(previous.feed_rate),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|line| format!("{line} ")).collect()
    }

    #[test]
    fn the_position_follows_absolute_and_relative_moves() {
        let lines = file(&["G21", "G0 X10 Y20 Z5 (Z9)", "G91", "G1 X1 Y-2 Z-1 F300", "G90 G0 X3", "G1 Y4 ; Y9"]);
        let at = |index| find_position_at(&lines, index).unwrap();
        assert_eq!(at(0), Position { x: 0.0, y: 0.0, z: 0.0, feed_rate: None });
        assert_eq!(at(4), Position { x: 11.0, y: 18.0, z: 4.0, feed_rate: Some(300.0) });
        assert_eq!(at(6), Position { x: 3.0, y: 4.0, z: 4.0, feed_rate: Some(300.0) });
    }

    #[test]
    fn commands_on_one_line_are_read_in_turn() {
        assert_eq!(split_at_g("G91 G1 X1"), ["GG", "G91 ", "GG", "G1 X1"]);
        assert_eq!(split_at_g("X1 G1"), ["GX1 ", "GG", "G1"]);
        let lines = file(&["G0 X10", "G91 G0 X1 G90 G0 Y2"]);
        assert_eq!(find_position_at(&lines, 2).unwrap().x, 11.0);
    }

    #[test]
    fn g90_1_and_g91_1_set_arc_centres_not_relative_moves() {
        let lines = file(&["G21", "G0 X10 Y10", "G71G91.1", "G1 X20", "G90.1 X99", "G91", "G90.1", "G1 X1"]);
        assert_eq!(find_position_at(&lines, 4).unwrap().x, 20.0);
        assert_eq!(find_position_at(&lines, 8).unwrap().x, 21.0);
    }

    #[test]
    fn unreadable_words_and_indices_past_the_end_are_errors() {
        assert_eq!(find_position_at(&file(&["G1 X2", "G1 X. Y2"]), 2), Err(GcodeError::Unreadable { index: 1 }));
        assert!(find_position_at(&file(&["G1 F"]), 1).is_err());
        assert_eq!(find_position_at(&file(&["G1"]), 3), Err(GcodeError::IndexOutOfRange { index: 3, len: 1 }));
    }

    #[test]
    fn a_streamed_lines_target_follows_from_the_last_one() {
        let start = Position { x: 1.0, y: 2.0, z: 3.0, feed_rate: None };
        let moved = line_target(start, Positioning::Absolute, "G1 X5 F300 ").unwrap();
        assert_eq!(moved, Position { x: 5.0, y: 2.0, z: 3.0, feed_rate: Some(300.0) });
        let relative = line_target(moved, Positioning::Relative, "G1 Y1 Z-1 ").unwrap();
        assert_eq!(relative, Position { x: 5.0, y: 3.0, z: 2.0, feed_rate: Some(300.0) });
        assert!(line_target(start, Positioning::Absolute, "G1 X. ").is_err());
        assert_eq!(word("G1 Z-1.5 ", 'Z'), Ok(Some(-1.5)));
    }

    #[test]
    fn lower_case_words_are_read_as_the_firmware_reads_them() {
        let start = Position { x: 1.0, y: 2.0, z: 3.0, feed_rate: None };
        let moved = line_target(start, Positioning::Absolute, "g1 x5 z-1 f300 ").unwrap();
        assert_eq!(moved, Position { x: 5.0, y: 2.0, z: -1.0, feed_rate: Some(300.0) });
        let lines = file(&["g0 x10 y20", "g91", "g1 x1 y1", "g90 g0 z2"]);
        assert_eq!(find_position_at(&lines, 4).unwrap(), Position { x: 11.0, y: 21.0, z: 2.0, feed_rate: None });
    }
}
