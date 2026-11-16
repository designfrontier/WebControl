//! Track B: what a camera centred over a point of the printed target sees, for tests and the synthetic camera.

use crate::detect::MM_PER_INCH;
use crate::grid::PITCH_MM;
use crate::{Frame, OpticalError, OpticalSettings, PixelPoint, Result};

/// Squares exist only on Python's 31 x 15 grid, so a camera off the sheet sees white paper.
const HALF_COLUMNS: f64 = 15.0;
const HALF_ROWS: f64 = 7.0;
const PAPER: f64 = 235.0;
const INK: f64 = 30.0;
/// Edge pixels take 16 x 16 samples, which keeps a square's centroid within 0.03 px.
const SAMPLES: usize = 16;

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

    /// The target as it should print: the printer scale is an error to measure, not to copy.
    pub fn nominal(settings: &OpticalSettings) -> Target {
        Target::from_settings(&OpticalSettings { scale_x: 1.0, scale_y: 1.0, ..settings.clone() })
    }

    /// The square nearest a point, clamped to the grid, as its centre in mm.
    pub fn nearest_square(&self, point_mm: (f64, f64)) -> (f64, f64) {
        let snap = |value: f64, pitch: f64, half: f64| (value / pitch).round().clamp(-half, half) * pitch;
        (snap(point_mm.0, self.pitch_mm.0, HALF_COLUMNS), snap(point_mm.1, self.pitch_mm.1, HALF_ROWS))
    }

    /// How far outside the nearest square a point is, per axis; both negative means inside.
    fn outside(&self, point_mm: (f64, f64)) -> (f64, f64) {
        let (x, y) = self.nearest_square(point_mm);
        ((point_mm.0 - x).abs() - self.marker_mm.0 / 2.0, (point_mm.1 - y).abs() - self.marker_mm.1 / 2.0)
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

impl Scene {
    /// The pixel under the camera's axis; pixel `n`'s centre is at `n`, as in OpenCV, so 640 wide gives 320.
    pub fn optical_centre(&self) -> PixelPoint {
        let centre = crate::measure::image_centre(self.width, self.height);
        PixelPoint { x: centre.x + self.centre_offset_px.0, y: centre.y + self.centre_offset_px.1 }
    }

    /// Where a point `offset_mm` from the camera (machine axes, +Y up) appears; +rotation turns it clockwise.
    pub fn to_pixel(&self, offset_mm: (f64, f64)) -> PixelPoint {
        let (a, b) = self.ideal(offset_mm);
        let gain = 1.0 + self.k() * (a * a + b * b);
        let centre = self.optical_centre();
        PixelPoint { x: centre.x + a * gain, y: centre.y + b * gain }
    }

    /// The inverse of [`Scene::to_pixel`].
    pub fn to_mm(&self, pixel: PixelPoint) -> (f64, f64) {
        let centre = self.optical_centre();
        let (u, v) = (pixel.x - centre.x, pixel.y - centre.y);
        let distorted = u.hypot(v);
        let radius = undistort(self.k(), distorted);
        let shrink = if distorted > 0.0 { radius / distorted } else { 1.0 };
        let (a, b) = (u * shrink / self.px_per_mm, v * shrink / self.px_per_mm);
        let (sin, cos) = self.rotation_deg.to_radians().sin_cos();
        let (x, y) = (a * cos + b * sin, b * cos - a * sin);
        (if self.mirrored { -x } else { x }, -y)
    }

    fn ideal(&self, (x, y): (f64, f64)) -> (f64, f64) {
        let (x, y) = ((if self.mirrored { -x } else { x }) * self.px_per_mm, -y * self.px_per_mm);
        let (sin, cos) = self.rotation_deg.to_radians().sin_cos();
        (x * cos - y * sin, x * sin + y * cos)
    }

    /// `radial_k1` is relative to half the longer side, so it means the same at any resolution.
    fn k(&self) -> f64 {
        let half = f64::from(self.width.max(self.height)) / 2.0;
        self.radial_k1 / (half * half)
    }

    fn corner_radius_sq(&self) -> f64 {
        let centre = self.optical_centre();
        let far = |size: u32, at: f64| at.abs().max((f64::from(size) - at).abs());
        far(self.width, centre.x).powi(2) + far(self.height, centre.y).powi(2)
    }

    fn check(&self) -> Result<()> {
        let invalid = |why: &str| Err(OpticalError::Invalid(format!("Synthetic scene: {why}")));
        let numbers = [self.px_per_mm, self.rotation_deg, self.radial_k1, self.blur_sigma, self.noise_sigma];
        let offsets = [self.centre_offset_px.0, self.centre_offset_px.1, self.light_gradient];
        if self.width == 0 || self.height == 0 {
            return invalid("the frame needs a width and a height");
        }
        if !numbers.iter().chain(&offsets).all(|n| n.is_finite()) || self.px_per_mm <= 0.0 {
            return invalid("px per mm must be positive and every number finite");
        }
        if self.blur_sigma < 0.0 || self.noise_sigma < 0.0 || !(0.0..=1.0).contains(&self.light_gradient) {
            return invalid("blur and noise must be at least 0 and the light gradient 0 to 1");
        }
        if 1.0 + 3.0 * self.k() * self.corner_radius_sq() < 0.5 {
            return invalid("radial_k1 bends the corners back on themselves");
        }
        Ok(())
    }
}

/// The undistorted radius `r` with `r (1 + k r²) = distorted`, by Newton's method.
fn undistort(k: f64, distorted: f64) -> f64 {
    if k == 0.0 {
        return distorted;
    }
    (0..8).fold(distorted, |r, _| r - (r + k * r * r * r - distorted) / (1.0 + 3.0 * k * r * r))
}

/// The frame with the optical centre over `camera_mm` (machine mm from the bed centre, +Y up).
pub fn render(target: &Target, scene: &Scene, camera_mm: (f64, f64)) -> Result<Frame> {
    scene.check()?;
    if !(target.marker_mm.0 > 0.0 && target.marker_mm.1 > 0.0 && target.pitch_mm.0 > 0.0 && target.pitch_mm.1 > 0.0) {
        return Err(OpticalError::Invalid("The target needs a positive square size and pitch".into()));
    }
    let (width, height) = (scene.width as usize, scene.height as usize);
    let coverage: Vec<f64> = (0..width * height)
        .map(|n| ink_coverage(target, scene, camera_mm, (n % width) as f64, (n / width) as f64))
        .collect();
    let lit: Vec<f64> = coverage
        .iter()
        .enumerate()
        .map(|(n, ink)| light(scene, n % width, n / width) * (PAPER - ink * (PAPER - INK)))
        .collect();
    let blurred = blur(&lit, width, height, scene.blur_sigma);
    let mut rng = Rng::new(scene.seed);
    let luma = blurred.iter().map(|value| (value + scene.noise_sigma * rng.normal()).round().clamp(0.0, 255.0) as u8);
    let frame = Frame { width: scene.width, height: scene.height, luma: luma.collect() };
    match scene.jpeg_quality {
        Some(quality) => jpeg_round_trip(&frame, quality),
        None => Ok(frame),
    }
}

/// The fraction of pixel (u, v) covered by ink; pixels well clear of an edge take one sample.
fn ink_coverage(target: &Target, scene: &Scene, camera_mm: (f64, f64), u: f64, v: f64) -> f64 {
    let at = |x: f64, y: f64| {
        let (dx, dy) = scene.to_mm(PixelPoint { x, y });
        target.outside((camera_mm.0 + dx, camera_mm.1 + dy))
    };
    let margin = 2.0 / scene.px_per_mm;
    let (ox, oy) = at(u, v);
    if ox.max(oy) > margin {
        return 0.0;
    }
    if ox.max(oy) < -margin {
        return 1.0;
    }
    let step = 1.0 / SAMPLES as f64;
    let inside = (0..SAMPLES * SAMPLES)
        .filter(|n| {
            let (sx, sy) = ((n % SAMPLES) as f64, (n / SAMPLES) as f64);
            let (ox, oy) = at(u - 0.5 + (sx + 0.5) * step, v - 0.5 + (sy + 0.5) * step);
            ox <= 0.0 && oy <= 0.0
        })
        .count();
    inside as f64 * step * step
}

/// A left-to-right fall-off plus vignetting, `light_gradient` at the strongest.
fn light(scene: &Scene, u: usize, v: usize) -> f64 {
    let (w, h) = (f64::from(scene.width), f64::from(scene.height));
    let r_sq = (u as f64 - w / 2.0).powi(2) + (v as f64 - h / 2.0).powi(2);
    1.0 - scene.light_gradient * (0.5 * u as f64 / w + 0.5 * r_sq / (w * w / 4.0 + h * h / 4.0))
}

/// A separable Gaussian blur with edges clamped.
fn blur(image: &[f64], width: usize, height: usize, sigma: f64) -> Vec<f64> {
    if sigma == 0.0 {
        return image.to_vec();
    }
    let radius = (3.0 * sigma).ceil() as isize;
    let weights: Vec<f64> = (-radius..=radius).map(|d| (-(d * d) as f64 / (2.0 * sigma * sigma)).exp()).collect();
    let total: f64 = weights.iter().sum();
    let pass = |source: &[f64], horizontal: bool| -> Vec<f64> {
        (0..width * height)
            .map(|n| {
                let (x, y) = ((n % width) as isize, (n / width) as isize);
                let tap = |d: isize| match horizontal {
                    true => source[y as usize * width + (x + d).clamp(0, width as isize - 1) as usize],
                    false => source[(y + d).clamp(0, height as isize - 1) as usize * width + x as usize],
                };
                (-radius..=radius).zip(&weights).map(|(d, w)| w * tap(d)).sum::<f64>() / total
            })
            .collect()
    };
    pass(&pass(image, true), false)
}

fn jpeg_round_trip(frame: &Frame, quality: u8) -> Result<Frame> {
    let failed = |why: image::ImageError| OpticalError::Invalid(format!("Synthetic JPEG: {why}"));
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality)
        .encode(&frame.luma, frame.width, frame.height, image::ExtendedColorType::L8)
        .map_err(failed)?;
    let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::Jpeg).map_err(failed)?;
    Ok(Frame { width: frame.width, height: frame.height, luma: decoded.to_luma8().into_raw() })
}

/// xorshift64* with Box-Muller, so the noise needs no `rand` and repeats on every platform.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed ^ 0x9E37_79B9_7F4A_7C15 | 1)
    }

    fn uniform(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        ((self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    }

    fn normal(&mut self) -> f64 {
        let (a, b) = (self.uniform(), self.uniform());
        (-2.0 * a.ln()).sqrt() * (std::f64::consts::TAU * b).cos()
    }
}
