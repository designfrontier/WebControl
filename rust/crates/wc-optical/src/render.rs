//! Track B: what a camera centred over a point of the printed target sees, for tests and the synthetic camera.

use crate::detect::MM_PER_INCH;
use crate::grid::PITCH_MM;
use crate::{Frame, OpticalSettings, Result, pending};

/// The printed target: square size and pitch in mm, after the printer scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Target {
    pub marker_mm: (f64, f64),
    pub pitch_mm: (f64, f64),
}

impl Target {
    pub fn from_settings(settings: &OpticalSettings) -> Target {
        Target {
            marker_mm: (settings.marker_x * MM_PER_INCH, settings.marker_y * MM_PER_INCH),
            pitch_mm: (PITCH_MM * settings.scale_x, PITCH_MM * settings.scale_y),
        }
    }
}

/// The camera and its defects; `seed` makes the noise repeatable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scene {
    pub width: u32,
    pub height: u32,
    pub px_per_mm: f64,
    pub rotation_deg: f64,
    pub mirrored: bool,
    pub centre_offset_px: (f64, f64),
    pub radial_k1: f64,
    pub blur_sigma: f64,
    pub noise_sigma: f64,
    pub light_gradient: f64,
    pub jpeg_quality: Option<u8>,
    pub seed: u64,
}

impl Default for Scene {
    fn default() -> Self {
        Scene {
            width: 640,
            height: 480,
            px_per_mm: 6.0,
            rotation_deg: 0.0,
            mirrored: false,
            centre_offset_px: (0.0, 0.0),
            radial_k1: 0.0,
            blur_sigma: 0.0,
            noise_sigma: 0.0,
            light_gradient: 0.0,
            jpeg_quality: None,
            seed: 1,
        }
    }
}

/// The frame with the image centre over `camera_mm` (machine mm from the bed centre, +Y up).
pub fn render(_target: &Target, _scene: &Scene, _camera_mm: (f64, f64)) -> Result<Frame> {
    pending("render::render")
}
