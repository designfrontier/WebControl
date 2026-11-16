//! The home shift (`GCodeFile.moveLine`/`_shiftAxis`): X and Y moved by the home position in exact decimal.

use crate::decimal::{Written, sum_text};
use crate::{Positioning, py, words};

/// The controller works in 32-bit floats, so digits past this are noise.
pub const MAX_SHIFT_DECIMALS: u32 = 6;

/// `data.gcodeShift` and the loaded file's truncate setting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HomeShift {
    pub x: f64,
    pub y: f64,
    pub truncate: Option<u32>,
}

impl HomeShift {
    /// The shift a load sets from the home position: each axis rounded to 4 places.
    pub fn from_home(home_x: f64, home_y: f64, truncate: Option<u32>) -> HomeShift {
        HomeShift { x: py::round(home_x, 4), y: py::round(home_y, 4), truncate }
    }

    fn places(&self) -> u32 {
        self.truncate.unwrap_or(MAX_SHIFT_DECIMALS)
    }
}

/// `moveLine`: the line upper-cased with X and Y shifted, any comment kept as written, or the line unchanged when
/// a coordinate cannot be read.
pub fn move_line(line: &str, shift: &HomeShift) -> String {
    if line.starts_with(['(', ';']) {
        return line.to_string();
    }
    let paren = line.find('(');
    let semicolon = line.find(';');
    let split = match (paren, semicolon) {
        (Some(paren), Some(semicolon)) if paren < semicolon => Some(paren),
        _ => semicolon,
    };
    let (code, comment) = line.split_at(split.unwrap_or(line.len()));
    let code = format!("{} ", py::upper(code));
    shift_axis(&code, 'X', shift.x, shift.places())
        .and_then(|code| shift_axis(&code, 'Y', shift.y, shift.places()))
        .map_or_else(|| line.to_string(), |code| code + comment)
}

fn shift_axis(line: &str, axis: char, shift: f64, cap: u32) -> Option<String> {
    let Some(word) = words::find(line, axis) else { return Some(line.to_string()) };
    let original = Written::parse(py::strip(word.text))?;
    let offset = Written::from_repr(&py::repr(shift))?;
    let places = original.places().max(offset.normalized_places()).min(cap);
    let text = sum_text(&original, &offset, places);
    Some(format!("{}{text}{}", &line[..=word.start], &line[word.end..]))
}

/// `_positioningModeSetBy`: the mode of the line's last G90 or G91, if it has one, in either case.
pub fn positioning_set_by(line: &str) -> Option<Positioning> {
    let line = line.to_ascii_uppercase();
    match (line.rfind("G90"), line.rfind("G91")) {
        (None, None) => None,
        (absolute, relative) if absolute > relative => Some(Positioning::Absolute),
        _ => Some(Positioning::Relative),
    }
}

/// `_shiftToHome`: the home shift applies only to a line that runs in absolute mode.
pub fn shift_to_home(line: &str, current: Positioning, shift: &HomeShift) -> String {
    match positioning_set_by(line).unwrap_or(current) {
        Positioning::Absolute => move_line(line, shift),
        Positioning::Relative => line.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shifted(line: &str, x: f64, y: f64, truncate: Option<u32>) -> String {
        move_line(line, &HomeShift { x, y, truncate })
    }

    #[test]
    fn the_users_line_is_sent_with_its_own_precision() {
        assert_eq!(
            shifted("G1 X-137.5730 Y41.3110 Z-9.529", 41.6763, 60.1447, None),
            "G1 X-95.8967 Y101.4557 Z-9.529 "
        );
    }

    #[test]
    fn no_shifted_number_carries_binary_rounding_digits() {
        let homes = [(41.6763, 60.1447), (0.1, 0.2), (-12.3456, 7.89), (123.4567, -45.6789)];
        let widest = homes
            .iter()
            .flat_map(|&(hx, hy)| {
                (-2_000_000..2_000_000).step_by(7919).flat_map(move |x| {
                    [-1234, 5, 98765].map(|y| {
                        let line = format!("G1 X{:.4} Y{:.3}", x as f64 / 10000.0, y as f64 / 1000.0);
                        shifted(&line, hx, hy, None)
                    })
                })
            })
            .flat_map(|line| {
                line.split(' ')
                    .filter(|word| word.starts_with(['X', 'Y']))
                    .map(|word| word.split_once('.').map_or(0, |(_, fraction)| fraction.len()))
                    .collect::<Vec<_>>()
            })
            .max();
        assert_eq!(widest, Some(4));
    }

    #[test]
    fn a_shifted_value_is_exact_in_decimal() {
        assert_eq!(shifted("G0 X10.1 Y0.2", 0.2, 0.1, None), "G0 X10.3 Y0.3 ");
    }

    #[test]
    fn whole_numbers_stay_whole_with_no_shift() {
        assert_eq!(shifted("G0 X10 Y-5", 0.0, 0.0, None), "G0 X10 Y-5 ");
    }

    #[test]
    fn the_truncate_option_still_caps_the_digits() {
        assert_eq!(shifted("G1 X-137.57301 Y41.31109", 41.6763, 60.1447, Some(2)), "G1 X-95.90 Y101.46 ");
    }

    #[test]
    fn more_than_six_decimals_are_capped() {
        assert_eq!(shifted("G1 X1.123456789", 0.0, 0.0, None), "G1 X1.123457 ");
    }

    #[test]
    fn zero_is_never_written_negative() {
        assert_eq!(shifted("G1 X-41.6763", 41.6763, 0.0, None), "G1 X0.0000 ");
        assert_eq!(shifted("G1 X-0", -0.0, 0.0, None), "G1 X0 ");
    }

    #[test]
    fn a_negative_value_that_rounds_to_zero_keeps_its_sign() {
        assert_eq!(shifted("G1 X-0.0000001", 0.0, 0.0, None), "G1 X-0.000000 ");
    }

    #[test]
    fn ties_round_to_even() {
        assert_eq!(shifted("G1 X0.125 Y0.135", 0.0, 0.0, Some(2)), "G1 X0.12 Y0.14 ");
    }

    #[test]
    fn comments_and_lines_without_coordinates_are_kept() {
        assert_eq!(shifted("G1 X1.5 (move) ; note", 1.0, 0.0, None), "G1 X2.5  (move) ; note");
        assert_eq!(shifted("(only a comment)", 1.0, 0.0, None), "(only a comment)");
        assert_eq!(shifted("G1 Z-1.25 F300", 1.0, 1.0, None), "G1 Z-1.25 F300 ");
    }

    #[test]
    fn a_paren_comment_without_a_semicolon_is_upper_cased_with_the_line() {
        assert_eq!(shifted("G1 x1 (move)", 1.0, 0.0, None), "G1 X2 (MOVE) ");
    }

    #[test]
    fn a_spaced_coordinate_is_shifted() {
        assert_eq!(shifted("G1 X 10.5", 1.25, 0.0, None), "G1 X11.75 ");
    }

    #[test]
    fn an_unreadable_coordinate_leaves_the_line_unchanged() {
        assert_eq!(shifted("G1 X. Y2", 1.0, 1.0, None), "G1 X. Y2");
        assert_eq!(shifted("g1 x1 Y-", 1.0, 1.0, None), "g1 x1 Y-");
    }

    #[test]
    fn tiny_and_huge_shifts_keep_their_digits() {
        assert_eq!(shifted("G1 X1 Y1", 1e-5, 100.0, None), "G1 X1.00001 Y101 ");
        assert_eq!(shifted("G1 X1", 1e30, 0.0, None), "G1 X1000000000000000000000000000000 ");
        assert_eq!(shifted("G1 X1", f64::NAN, 0.0, None), "G1 X1");
    }

    #[test]
    fn numbers_longer_than_28_digits_are_rounded_like_python() {
        assert_eq!(shifted("G1 X32.123456789012345678901234567", 0.0, 0.0, None), "G1 X32.123457 ");
        assert_eq!(shifted("G1 X0.99999999999999999999999999999", 0.0, 0.0, None), "G1 X1.000000 ");
        assert_eq!(
            shifted("G1 X123456789012345678901234567890", 0.0, 0.0, None),
            "G1 X123456789012345678901234567900 "
        );
    }

    #[test]
    fn a_load_rounds_home_to_four_places() {
        assert_eq!(
            HomeShift::from_home(12.34567, -1.00004, Some(2)),
            HomeShift { x: 12.3457, y: -1.0, truncate: Some(2) }
        );
    }

    #[test]
    fn the_last_g90_or_g91_sets_the_mode() {
        assert_eq!(positioning_set_by("G1 X1"), None);
        assert_eq!(positioning_set_by("G90 G91 X1"), Some(Positioning::Relative));
        assert_eq!(positioning_set_by("G91 G90 X1"), Some(Positioning::Absolute));
        assert_eq!(positioning_set_by("G91.1"), Some(Positioning::Relative));
        assert_eq!(positioning_set_by("g91 g0 x1"), Some(Positioning::Relative));
        assert_eq!(positioning_set_by("g91 g90"), Some(Positioning::Absolute));
    }

    #[test]
    fn only_absolute_lines_are_shifted() {
        let shift = HomeShift { x: 1.0, y: 1.0, truncate: None };
        assert_eq!(shift_to_home("G1 X1", Positioning::Absolute, &shift), "G1 X2 ");
        assert_eq!(shift_to_home("G1 X1", Positioning::Relative, &shift), "G1 X1");
        assert_eq!(shift_to_home("G90 G1 X1", Positioning::Relative, &shift), "G90 G1 X2 ");
        assert_eq!(shift_to_home("G91 G1 X1", Positioning::Absolute, &shift), "G91 G1 X1");
        assert_eq!(shift_to_home("g90 g1 x1", Positioning::Relative, &shift), "G90 G1 X2 ");
    }
}
