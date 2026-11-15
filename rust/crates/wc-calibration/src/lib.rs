//! Holey and triangular calibration, ported from `Actions/holeyCalibration.py` and
//! `Actions/triangularCalibration.py`: the fits, the test patterns and what Accept saves and sends. Pure: the
//! caller supplies the settings and acts on the result. Checked against `tests/fixtures/calibration/`.

mod accept;
pub mod holey;
mod python;
mod settings;
mod trf;
pub mod triangular;

use std::fmt;

pub use accept::{Accept, SettingWrite};
pub use settings::{MachineSettings, SettingsError};
use wc_config::python_float_str;

/// Why a calculation was refused; `Display` gives the message Python shows for it.
#[derive(Debug, Clone, PartialEq)]
pub enum CalibrationError {
    /// The field, as Python names it ("the distance M5").
    NotANumber(String),
    OutOfRange {
        what: &'static str,
        value: f64,
    },
    NoSolution,
    NotCalibrated,
    /// A value Accept would save or send is NaN or infinite.
    NotFinite,
}

impl fmt::Display for CalibrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CalibrationError::NotANumber(field) => write!(f, "Please enter a number for {field}."),
            CalibrationError::OutOfRange { what, value } => write!(
                f,
                "The {what} of {} mm seems wrong.\n\nPlease check the number and enter it again.",
                python_float_str(*value)
            ),
            CalibrationError::NoSolution => {
                write!(f, "The calibration could not be calculated from these measurements.")
            }
            CalibrationError::NotFinite => {
                write!(f, "The calibration result is not a finite number, so it was not saved. Calculate it again.")
            }
            CalibrationError::NotCalibrated => write!(
                f,
                "The machine was not able to be calibrated. Please ensure the work area dimensions are correct and try again."
            ),
        }
    }
}

impl std::error::Error for CalibrationError {}
