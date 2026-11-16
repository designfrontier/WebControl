//! `GCodeFile.loadUpdateFile`: the text filter that turns a file into the lines WebControl streams, and `zMoves`.

use std::sync::LazyLock;

use regex::Regex;

use crate::{GcodeError, py, words};

static WORD_AFTER_DIGIT: LazyLock<Regex> = LazyLock::new(|| Regex::new("(?i)([0-9])([GXYZIJFTM]) *").expect("pattern"));
static SPACE_RUNS: LazyLock<Regex> = LazyLock::new(|| Regex::new("  +").expect("pattern"));

/// Python's universal newlines, which `open(path, "r")` applies when the route reads a file from disk.
pub fn universal_newlines(raw: &str) -> String {
    raw.replace("\r\n", "\n").replace('\r', "\n")
}

/// The lines of `raw` as WebControl streams them: spaced words, single spaces, an optional truncation of long
/// decimals to `truncate` digits, no leading space, and one trailing space.
pub fn filter(raw: &str, truncate: Option<usize>) -> Result<Vec<String>, GcodeError> {
    let text = raw.replace("\n\n", "\n");
    let text = WORD_AFTER_DIGIT.replace_all(&text, "$1 $2");
    let mut text = SPACE_RUNS.replace_all(&text, " ").into_owned();
    if let Some(digits) = truncate {
        if digits == 0 {
            return Err(GcodeError::TruncateDigits);
        }
        let pattern = Regex::new(&format!(r"([+-]?\d*\.\d{{1,{digits}}})(\d*)")).expect("pattern");
        text = pattern.replace_all(&text, "$1").into_owned();
    }
    Ok(text.split('\n').map(spaced_line).collect())
}

fn spaced_line(line: &str) -> String {
    ["X ", "Y ", "Z ", "I ", "J ", "F "]
        .iter()
        .fold(format!("{} ", py::lstrip(line)), |line, word| line.replace(word, &word[..1]))
}

/// The indices where the file's Z changes by more than the tolerance, for the Z up/down index buttons.
#[derive(Debug, Clone, PartialEq)]
pub struct ZMoves {
    pub indices: Vec<usize>,
    /// The tolerance after the file's last G20/G21, which Python also stores as `Computed Settings.tolerance`.
    pub tolerance: f64,
    /// The console lines Python logs for Z words it could not read.
    pub warnings: Vec<String>,
}

pub fn z_moves(lines: &[String], tolerance: f64) -> ZMoves {
    let mut moves = ZMoves { indices: vec![0], tolerance, warnings: Vec::new() };
    let mut last_z = None;
    lines.iter().enumerate().for_each(|(index, line)| {
        let line = words::code(line);
        if !py::isspace(&line) {
            if line.contains("G20") {
                moves.tolerance = 0.020;
            }
            if line.contains("G21") {
                moves.tolerance = 0.50;
            }
            match words::value(&line, 'Z') {
                Ok(Some(z)) => {
                    if last_z.is_none_or(|last: f64| (z - last).abs() > moves.tolerance) {
                        moves.indices.push(index);
                    }
                    last_z = Some(z);
                }
                Ok(None) => {}
                Err(()) => moves.warnings.push(unreadable_word('Z', &line)),
            }
        }
    });
    moves
}

pub(crate) fn unreadable_word(letter: char, line: &str) -> String {
    format!("Preview skipped an unreadable {letter} word: {}", py::strip(line))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(raw: &str) -> Vec<String> {
        filter(raw, None).unwrap()
    }

    #[test]
    fn words_are_spaced_and_lines_end_in_one_space() {
        assert_eq!(lines("G1X10.5Y2Z-1F300\n"), ["G1 X10.5 Y2 Z-1 F300 ", " "]);
        assert_eq!(lines("  G0   X 1  Y 2"), ["G0 X1 Y2 "]);
        assert_eq!(lines("G1 X1T2M6"), ["G1 X1 T2 M6 "]);
        assert_eq!(lines("g1x1m3"), ["g1 x1 m3 "], "the firmware upper-cases it, so it is spaced as upper case");
    }

    #[test]
    fn only_pairs_of_blank_lines_are_squeezed() {
        assert_eq!(lines("a\n\nb\n\n\nc\n\n\n\nd"), ["a ", "b ", " ", "c ", " ", "d "]);
    }

    #[test]
    fn truncate_cuts_long_decimals() {
        assert_eq!(filter("G1 X1.23456 Y-0.5 Z.987654", Some(2)).unwrap(), ["G1 X1.23 Y-0.5 Z.98 "]);
        assert_eq!(filter("G1 X1", Some(0)), Err(GcodeError::TruncateDigits));
    }

    #[test]
    fn universal_newlines_end_every_line_with_a_newline() {
        assert_eq!(universal_newlines("a\r\nb\rc\n"), "a\nb\nc\n");
    }

    #[test]
    fn z_moves_mark_each_change_beyond_the_tolerance() {
        let file = lines("G21\nG0 Z5\nG1 Z4.8\nG1 Z4\n(Z9)\nG1 X1 Z4.2 ; Z9\nG20\nG1 Z4.21\nG1 Z4.25\nG1 Z`3");
        let moves = z_moves(&file, 0.5);
        assert_eq!(moves.indices, [0, 1, 3, 8]);
        assert_eq!(moves.tolerance, 0.020);
        assert_eq!(moves.warnings, ["Preview skipped an unreadable Z word: G1 Z`3"]);
    }

    #[test]
    fn g21_wins_when_a_line_has_both_units() {
        assert_eq!(z_moves(&lines("G20 G21"), 0.1).tolerance, 0.5);
        assert_eq!(
            z_moves(&lines("g20\ng1 z1"), 0.5),
            ZMoves { indices: vec![0, 1], tolerance: 0.020, warnings: vec![] }
        );
    }
}
