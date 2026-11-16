//! Track A: one frame's square, by the §1.5 pipeline (blur, Canny, close, contours, min-area rectangle).

use crate::{Frame, OpticalSettings, PixelPoint, Result, pending};

pub const MM_PER_INCH: f64 = 25.4;
/// Python ignores contours smaller than this.
pub const MIN_AREA_PX: f64 = 1000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Calibrate,
    FindCentre,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectParams {
    pub blur_kernel: u32,
    pub canny_low: f32,
    pub canny_high: f32,
    pub min_area_px: f64,
    /// The printed square's size, after the printer scale.
    pub marker_mm: (f64, f64),
    pub mode: Mode,
}

impl DetectParams {
    pub fn from_settings(settings: &OpticalSettings, mode: Mode) -> DetectParams {
        DetectParams {
            blur_kernel: settings.gaussian_blur_value.max(1) as u32,
            canny_low: settings.canny_low_value as f32,
            canny_high: settings.canny_high_value as f32,
            min_area_px: MIN_AREA_PX,
            marker_mm: (settings.marker_x * MM_PER_INCH, settings.marker_y * MM_PER_INCH),
            mode,
        }
    }
}

/// The largest square in a frame: corners ordered top-left, top-right, bottom-right, bottom-left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Square {
    pub centre: PixelPoint,
    pub corners: [PixelPoint; 4],
    pub angle_deg: f64,
    pub px_per_mm: (f64, f64),
}

pub fn find_square(_frame: &Frame, _params: &DetectParams) -> Result<Square> {
    pending("detect::find_square")
}
