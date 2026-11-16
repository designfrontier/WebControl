//! Track F: the holey geometry (motor spacing, motorOffsetY, chain tolerances) and the target sheet's pose that best
//! explain an optical error grid, solved with `trf.rs`.

use wc_config::Settings;
use wc_protocol::optical::{Cell, GeometryFit};

use crate::CalibrationError;

/// `errors` holds each measured square's (x, y) error in mm, as `wc_optical::grid::ErrorGrid::measured` lists them.
pub fn fit(_settings: &Settings, _errors: &[(Cell, (f64, f64))]) -> Result<GeometryFit, CalibrationError> {
    Err(CalibrationError::NotImplemented("The optical geometry fit"))
}
