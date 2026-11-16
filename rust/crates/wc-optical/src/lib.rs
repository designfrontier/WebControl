//! Experimental optical calibration (`spec/optical-calibration.md`), sans IO apart from the camera adapters.
//! Step 0 froze these signatures; each stub answers [`OpticalError::NotImplemented`] until its track lands.

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
    /// A stub left by step 0; names the function.
    NotImplemented(&'static str),
    NoFrame,
    NoSquare,
    Camera(String),
    Invalid(String),
}

impl fmt::Display for OpticalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpticalError::NotImplemented(what) => write!(f, "{what} is not implemented yet"),
            OpticalError::NoFrame => f.write_str("The camera gave no frame"),
            OpticalError::NoSquare => f.write_str("No square found in the frame"),
            OpticalError::Camera(why) => write!(f, "Camera: {why}"),
            OpticalError::Invalid(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for OpticalError {}

pub type Result<T> = std::result::Result<T, OpticalError>;

pub(crate) fn pending<T>(what: &'static str) -> Result<T> {
    Err(OpticalError::NotImplemented(what))
}
