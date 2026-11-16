//! Track A: a square's offset over several frames (the ±2σ cut), the optical centre and the camera's mounting.

use crate::{Frame, Measurement, OpticalSettings, Orientation, PixelPoint, Result, pending};

/// Frames averaged per measurement, as Python.
pub const FRAMES: usize = 10;

/// The offset of the square under the camera in machine mm; `orientation` replaces the angle heuristic.
pub fn measure(
    _frames: &[Frame],
    _settings: &OpticalSettings,
    _orientation: Option<&Orientation>,
) -> Result<Measurement> {
    pending("measure::measure")
}

/// The averaged pixel centre of the square, offered as the new `opticalCenterX/Y`.
pub fn find_centre(_frames: &[Frame], _settings: &OpticalSettings) -> Result<PixelPoint> {
    pending("measure::find_centre")
}

/// From the square's pixel centre before and after a `step_mm` move in +X, then in +Y.
pub fn orientation(
    _start: PixelPoint,
    _after_x: PixelPoint,
    _after_y: PixelPoint,
    _step_mm: f64,
) -> Result<Orientation> {
    pending("measure::orientation")
}
