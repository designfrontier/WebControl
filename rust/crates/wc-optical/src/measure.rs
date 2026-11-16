//! Track A: a square's offset over several frames (the ±2σ cut), the optical centre and the camera's mounting.
//!
//! The mounting maps a square's machine offset `m` (mm, +Y up) to its image offset `w` (px, +y up) as
//! `w = px_per_mm · R(rotation) · F · m`, where `F` flips x when the camera is mirrored.

use crate::detect::{self, DetectParams, Mode, Square};
use crate::{Frame, Measurement, OpticalError, OpticalSettings, Orientation, PixelPoint, Result};

/// Frames averaged per measurement, as Python.
pub const FRAMES: usize = 10;

/// The middle pixel in OpenCV terms, where pixel `n`'s centre is at `n`: (320, 240) for 640x480, as Python's
/// `int(width / 2)`. The renderer and the detector share this convention.
pub fn image_centre(width: u32, height: u32) -> PixelPoint {
    PixelPoint { x: f64::from(width) / 2.0, y: f64::from(height) / 2.0 }
}

/// The settings' optical centre, or the image centre while either coordinate is 0 (Python's convention).
pub fn optical_centre(settings: &OpticalSettings, width: u32, height: u32) -> PixelPoint {
    if settings.optical_center_x == 0.0 || settings.optical_center_y == 0.0 {
        image_centre(width, height)
    } else {
        PixelPoint { x: settings.optical_center_x, y: settings.optical_center_y }
    }
}

/// The squares found in at least half the frames, with the frame size, or why not.
fn detect_all(frames: &[Frame], params: &DetectParams) -> Result<(Vec<Square>, u32, u32)> {
    let first = frames.first().ok_or(OpticalError::NoFrame)?;
    let squares = frames.iter().filter_map(|f| detect::find_square(f, params).ok()).collect::<Vec<_>>();
    if squares.is_empty() || squares.len() * 2 < frames.len() {
        return Err(OpticalError::NoSquare);
    }
    Ok((squares, first.width, first.height))
}

/// Keeps the samples whose first two values each lie within two population deviations of their mean, as Python's
/// cut did; the other values ride along.
fn within_two_sigma<const N: usize>(samples: Vec<[f64; N]>) -> Vec<[f64; N]> {
    let n = samples.len() as f64;
    let mean = |k: usize| samples.iter().map(|s| s[k]).sum::<f64>() / n;
    let bounds = (0..2)
        .map(|k| {
            let m = mean(k);
            let sd = (samples.iter().map(|s| (s[k] - m).powi(2)).sum::<f64>() / n).sqrt();
            (m - 2.0 * sd, m + 2.0 * sd)
        })
        .collect::<Vec<_>>();
    samples.iter().filter(|s| bounds.iter().zip(s.iter()).all(|((lo, hi), v)| lo <= v && v <= hi)).copied().collect()
}

fn average<const N: usize>(samples: &[[f64; N]]) -> [f64; N] {
    std::array::from_fn(|k| samples.iter().map(|s| s[k]).sum::<f64>() / samples.len() as f64)
}

/// The square's offset in machine mm: undo the mounting, then scale by the marker's measured size.
fn offset_mm(square: &Square, centre: PixelPoint, rotation_deg: f64, mirrored: bool) -> (f64, f64) {
    let w = (square.centre.x - centre.x, centre.y - square.centre.y);
    let (sin, cos) = (-rotation_deg).to_radians().sin_cos();
    let unrotated = (w.0 * cos - w.1 * sin, w.0 * sin + w.1 * cos);
    let flip = if mirrored { -1.0 } else { 1.0 };
    (flip * unrotated.0 / square.px_per_mm.0, unrotated.1 / square.px_per_mm.1)
}

/// The offset of the square under the camera in machine mm; `orientation` replaces the angle heuristic. Without
/// it, the square's own tilt is taken as the camera's rotation, as Python intended.
pub fn measure(frames: &[Frame], settings: &OpticalSettings, orientation: Option<&Orientation>) -> Result<Measurement> {
    let (squares, width, height) = detect_all(frames, &DetectParams::from_settings(settings, Mode::Calibrate))?;
    let centre = optical_centre(settings, width, height);
    let samples = squares
        .iter()
        .map(|square| {
            let (rotation, mirrored) = orientation.map_or((square.angle_deg, false), |o| (o.rotation_deg, o.mirrored));
            let (dx, dy) = offset_mm(square, centre, rotation, mirrored);
            [dx, dy, square.centre.x, square.centre.y, square.angle_deg]
        })
        .collect();
    let kept = within_two_sigma(samples);
    let [dx_mm, dy_mm, x, y, angle_deg] = average(&kept);
    Ok(Measurement { dx_mm, dy_mm, centre_px: PixelPoint { x, y }, angle_deg, frames_used: kept.len() as u32 })
}

/// The averaged pixel centre of the square, offered as the new `opticalCenterX/Y`.
pub fn find_centre(frames: &[Frame], settings: &OpticalSettings) -> Result<PixelPoint> {
    let (squares, _, _) = detect_all(frames, &DetectParams::from_settings(settings, Mode::FindCentre))?;
    let [x, y] = average(&within_two_sigma(squares.iter().map(|s| [s.centre.x, s.centre.y]).collect()));
    Ok(PixelPoint { x, y })
}

/// From the square's pixel centre before and after a `step_mm` move in +X, then in +Y.
pub fn orientation(start: PixelPoint, after_x: PixelPoint, after_y: PixelPoint, step_mm: f64) -> Result<Orientation> {
    if !(step_mm.is_finite() && step_mm > 0.0) {
        return Err(OpticalError::Invalid(format!("The orientation step must be positive, got {step_mm}")));
    }
    // Moving the camera +X moves the square -X relative to it; columns are image +y up per machine mm.
    let x_col = ((start.x - after_x.x) / step_mm, (after_x.y - start.y) / step_mm);
    let y_col = ((after_x.x - after_y.x) / step_mm, (after_y.y - after_x.y) / step_mm);
    let det = x_col.0 * y_col.1 - x_col.1 * y_col.0;
    if det.abs() < 1e-6 {
        return Err(OpticalError::Invalid("The square did not move with the machine".into()));
    }
    let mirrored = det < 0.0;
    let x_col = if mirrored { (-x_col.0, -x_col.1) } else { x_col };
    let (cos, sin) = ((x_col.0 + y_col.1) / 2.0, (x_col.1 - y_col.0) / 2.0);
    Ok(Orientation { rotation_deg: sin.atan2(cos).to_degrees(), mirrored, px_per_mm: cos.hypot(sin) })
}
