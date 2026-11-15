//! G-code as the Python backend reads, rewrites and previews it (`File/gcodeFile.py`, `Connection/firmwareGcode.py`,
//! the restart half of `Actions/actions.py`), and the board model (`Boards/`). Pure functions, no IO: every string
//! that reaches the controller or a shared file matches Python's byte for byte (tests/fixtures/gcode).

mod decimal;
pub mod firmware_gcode;
pub mod load;
pub mod py;
pub mod sanitise;
pub mod shift;
mod words;

use std::fmt;

pub use wc_protocol::main::Units;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Positioning {
    Absolute,
    Relative,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GcodeError {
    /// A coordinate or feed word on this 0-based line cannot be read as a number.
    Unreadable {
        index: usize,
    },
    IndexOutOfRange {
        index: usize,
        len: usize,
    },
    /// Truncation to 0 digits, which Python's pattern refuses.
    TruncateDigits,
}

impl fmt::Display for GcodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GcodeError::Unreadable { index } => write!(f, "line {} has a word that is not a number", index + 1),
            GcodeError::IndexOutOfRange { index, len } => write!(f, "line index {index} is past the {len} lines"),
            GcodeError::TruncateDigits => f.write_str("truncate needs at least 1 digit"),
        }
    }
}

impl std::error::Error for GcodeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_say_which_line() {
        assert_eq!(GcodeError::Unreadable { index: 4 }.to_string(), "line 5 has a word that is not a number");
        assert_eq!(GcodeError::IndexOutOfRange { index: 9, len: 3 }.to_string(), "line index 9 is past the 3 lines");
        assert_eq!(GcodeError::TruncateDigits.to_string(), "truncate needs at least 1 digit");
    }
}
