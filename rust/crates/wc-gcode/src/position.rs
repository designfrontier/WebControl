//! `Actions.findPositionAt`: the file's X, Y and Z target before a line, for `gcodePositionMessage`.

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
        let line = words::blank_comments(raw);
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
    if line.contains("G90") {
        *positioning = Positioning::Absolute;
    }
    if line.contains("G91") {
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
    fn unreadable_words_and_indices_past_the_end_are_errors() {
        assert_eq!(find_position_at(&file(&["G1 X2", "G1 X. Y2"]), 2), Err(GcodeError::Unreadable { index: 1 }));
        assert!(find_position_at(&file(&["G1 F"]), 1).is_err());
        assert_eq!(find_position_at(&file(&["G1"]), 3), Err(GcodeError::IndexOutOfRange { index: 3, len: 1 }));
    }
}
