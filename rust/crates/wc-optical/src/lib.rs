//! Experimental optical calibration (`spec/optical-calibration.md`), sans IO apart from the camera adapters.

pub mod camera;
pub mod codec;
pub mod detect;
pub mod frame;
pub mod grid;
pub mod measure;
pub mod overlay;
pub mod render;
pub mod target;

use std::fmt;

pub use frame::Frame;
pub use wc_protocol::optical::{Cell, Measurement, OpticalSettings, Orientation, Paper, PixelPoint};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpticalError {
    NoFrame,
    NoSquare,
    Camera(String),
    Invalid(String),
}

impl fmt::Display for OpticalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpticalError::NoFrame => f.write_str("The camera gave no frame"),
            OpticalError::NoSquare => f.write_str("No square found in the frame"),
            OpticalError::Camera(why) => write!(f, "Camera: {why}"),
            OpticalError::Invalid(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for OpticalError {}

pub type Result<T> = std::result::Result<T, OpticalError>;
