//! Track A: one frame's square. The §1.5 pipeline (blur, Canny, close, contours, min-area rectangle) finds it
//! roughly; each side is then located to sub-pixel accuracy from its edge profiles and the corners intersected.

use image::GrayImage;
use imageproc::contours::{BorderType, Contour, find_contours};
use imageproc::distance_transform::Norm;
use imageproc::edges::canny;
use imageproc::filter::gaussian_blur_f32;
use imageproc::geometry::{contour_area, min_area_rect};
use imageproc::morphology::close;

use crate::{Frame, OpticalError, OpticalSettings, PixelPoint, Result};

pub const MM_PER_INCH: f64 = 25.4;
/// Python ignores contours smaller than this.
pub const MIN_AREA_PX: f64 = 1000.0;
/// A contour must fill this much of its rectangle, which rejects open edges and partial squares.
const MIN_FILL: f64 = 0.85;
const MIN_CONTRAST: f64 = 24.0;
const MIN_EDGE_POINTS: usize = 8;
const PROFILE_STEP: f64 = 0.5;
/// Samples at each end of a profile that give its dark and light levels.
const LEVEL_SAMPLES: usize = 4;

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
/// `angle_deg` is its top edge's counter-clockwise tilt on screen, in (-45, 45].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Square {
    pub centre: PixelPoint,
    pub corners: [PixelPoint; 4],
    pub angle_deg: f64,
    pub px_per_mm: (f64, f64),
}

type P = (f64, f64);

fn sub(a: P, b: P) -> P {
    (a.0 - b.0, a.1 - b.1)
}

fn along(base: P, dir: P, t: f64) -> P {
    (base.0 + dir.0 * t, base.1 + dir.1 * t)
}

fn cross(a: P, b: P) -> f64 {
    a.0 * b.1 - a.1 * b.0
}

fn mid(a: P, b: P) -> P {
    ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0)
}

fn dist(a: P, b: P) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// OpenCV's sigma for a kernel size when it is given as 0.
fn kernel_sigma(kernel: u32) -> f32 {
    0.3 * ((kernel as f32 - 1.0) * 0.5 - 1.0) + 0.8
}

fn touches_border(contour: &Contour<i32>, width: u32, height: u32) -> bool {
    let (w, h) = (width as i32, height as i32);
    contour.points.iter().any(|p| p.x <= 1 || p.y <= 1 || p.x >= w - 2 || p.y >= h - 2)
}

/// Corners sorted clockwise on screen, starting where the side to the next corner is nearest left-to-right.
fn order(corners: [P; 4]) -> [P; 4] {
    let centre = corners.iter().fold((0.0, 0.0), |acc, c| (acc.0 + c.0 / 4.0, acc.1 + c.1 / 4.0));
    let angle = |c: &P| (c.1 - centre.1).atan2(c.0 - centre.0);
    let mut sorted = corners;
    sorted.sort_by(|a, b| angle(a).total_cmp(&angle(b)));
    let tilt = |k: usize| {
        let side = sub(sorted[(k + 1) % 4], sorted[k]);
        side.1.atan2(side.0).abs()
    };
    let first = (0..4).min_by(|a, b| tilt(*a).total_cmp(&tilt(*b))).unwrap_or(0);
    sorted.rotate_left(first);
    sorted
}

/// 2x2 box average; a pixel centre `c` here sits at `2c + 0.5` in the source.
fn halve(gray: &GrayImage) -> GrayImage {
    GrayImage::from_fn(gray.width() / 2, gray.height() / 2, |x, y| {
        let sum = [(0, 0), (1, 0), (0, 1), (1, 1)].iter().map(|(dx, dy)| u32::from(gray[(2 * x + dx, 2 * y + dy)][0]));
        image::Luma([((sum.sum::<u32>() + 2) / 4) as u8])
    })
}

/// Frames this wide find the square at half size; Canny dominates and the sides are refined at full size anyway.
const HALVE_FROM_WIDTH: u32 = 640;

fn rough(gray: &GrayImage, params: &DetectParams) -> Result<[P; 4]> {
    if gray.width() < HALVE_FROM_WIDTH {
        return rough_at(gray, params);
    }
    let small = DetectParams { min_area_px: params.min_area_px / 4.0, ..*params };
    rough_at(&halve(gray), &small).map(|corners| corners.map(|(x, y)| (2.0 * x + 0.5, 2.0 * y + 0.5)))
}

/// The §1.5 pipeline: the min-area rectangle of the largest filled contour away from the border.
fn rough_at(gray: &GrayImage, params: &DetectParams) -> Result<[P; 4]> {
    let blurred =
        if params.blur_kernel > 1 { gaussian_blur_f32(gray, kernel_sigma(params.blur_kernel)) } else { gray.clone() };
    let (low, high) = (params.canny_low.max(0.0), params.canny_high.max(0.0));
    let edges = close(&canny(&blurred, low.min(high), low.max(high)), Norm::LInf, 1);
    find_contours::<i32>(&edges)
        .iter()
        .filter(|c| c.border_type == BorderType::Outer && c.parent.is_none())
        .filter(|c| !touches_border(c, gray.width(), gray.height()))
        .map(|c| (contour_area(&c.points), min_area_rect(&c.points)))
        .filter(|(area, rect)| *area > params.min_area_px && *area >= MIN_FILL * contour_area(rect))
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, rect)| order(rect.map(|p| (f64::from(p.x), f64::from(p.y)))))
        .ok_or(OpticalError::NoSquare)
}

fn bilinear(frame: &Frame, (x, y): P) -> Option<f64> {
    let (w, h) = (frame.width as usize, frame.height as usize);
    if !(x >= 0.0 && y >= 0.0 && x <= (w - 1) as f64 && y <= (h - 1) as f64) {
        return None;
    }
    let (x0, y0) = ((x as usize).min(w - 2), (y as usize).min(h - 2));
    let (fx, fy) = (x - x0 as f64, y - y0 as f64);
    let at = |cx: usize, cy: usize| f64::from(frame.luma[cy * w + cx]);
    let top = at(x0, y0) * (1.0 - fx) + at(x0 + 1, y0) * fx;
    let bottom = at(x0, y0 + 1) * (1.0 - fx) + at(x0 + 1, y0 + 1) * fx;
    Some(top * (1.0 - fy) + bottom * fy)
}

/// Where a dark-to-light edge crosses the outward `normal` through `base`: the dark area under the profile,
/// which blur and pixel coverage both preserve.
fn edge_offset(frame: &Frame, base: P, normal: P, reach: f64) -> Option<f64> {
    let count = (2.0 * reach / PROFILE_STEP) as usize;
    let profile = (0..count)
        .map(|k| bilinear(frame, along(base, normal, -reach + (k as f64 + 0.5) * PROFILE_STEP)))
        .collect::<Option<Vec<f64>>>()?;
    let mean = |values: &[f64]| values.iter().sum::<f64>() / values.len() as f64;
    let dark = mean(&profile[..LEVEL_SAMPLES]);
    let light = mean(&profile[count - LEVEL_SAMPLES..]);
    if light - dark < MIN_CONTRAST {
        return None;
    }
    Some(-reach + profile.iter().map(|v| (light - v) / (light - dark)).sum::<f64>() * PROFILE_STEP)
}

/// Edge points along the middle 70% of the side from `a` to `b`, clockwise on screen so outward is to the left.
fn edge_points(frame: &Frame, a: P, b: P) -> Vec<P> {
    let length = dist(a, b);
    let dir = (sub(b, a).0 / length, sub(b, a).1 / length);
    let normal = (dir.1, -dir.0);
    let reach = (0.15 * length).clamp(5.0, 15.0);
    let samples = (0.7 * length) as usize;
    (0..samples)
        .filter_map(|k| {
            let base = along(a, dir, length * (0.15 + 0.7 * (k as f64 + 0.5) / samples as f64));
            edge_offset(frame, base, normal, reach).map(|e| along(base, normal, e))
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
struct Line {
    point: P,
    dir: P,
}

/// Total least squares.
fn fit(points: &[P]) -> Line {
    let n = points.len() as f64;
    let m = points.iter().fold((0.0, 0.0), |acc, p| (acc.0 + p.0 / n, acc.1 + p.1 / n));
    let (sxx, sxy, syy) = points
        .iter()
        .map(|p| sub(*p, m))
        .fold((0.0, 0.0, 0.0), |acc, d| (acc.0 + d.0 * d.0, acc.1 + d.0 * d.1, acc.2 + d.1 * d.1));
    let angle = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    Line { point: m, dir: (angle.cos(), angle.sin()) }
}

fn residual(line: &Line, p: P) -> f64 {
    cross(sub(p, line.point), line.dir).abs()
}

/// Two rounds of dropping points beyond three robust deviations, so an occluded stretch does not bend the side.
fn robust_fit(points: Vec<P>) -> Option<Line> {
    let prune = |points: Vec<P>| -> Option<Vec<P>> {
        (points.len() >= MIN_EDGE_POINTS).then_some(())?;
        let line = fit(&points);
        let mut residuals = points.iter().map(|p| residual(&line, *p)).collect::<Vec<_>>();
        residuals.sort_by(f64::total_cmp);
        let limit = (3.0 * 1.4826 * residuals[residuals.len() / 2]).max(0.25);
        Some(points.into_iter().filter(|p| residual(&line, *p) <= limit).collect())
    };
    let kept = prune(prune(points)?)?;
    (kept.len() >= MIN_EDGE_POINTS).then(|| fit(&kept))
}

fn intersect(a: &Line, b: &Line) -> Option<P> {
    let det = cross(a.dir, b.dir);
    (det.abs() > 1e-9).then(|| along(a.point, a.dir, cross(sub(b.point, a.point), b.dir) / det))
}

fn refine(frame: &Frame, corners: [P; 4]) -> Option<[P; 4]> {
    let sides = (0..4)
        .map(|k| robust_fit(edge_points(frame, corners[k], corners[(k + 1) % 4])))
        .collect::<Option<Vec<Line>>>()?;
    let corner = |k: usize| intersect(&sides[(k + 3) % 4], &sides[k]);
    let refined = [corner(0)?, corner(1)?, corner(2)?, corner(3)?];
    let size = dist(corners[0], corners[2]);
    refined.iter().zip(corners).all(|(r, c)| dist(*r, c) < 0.2 * size).then_some(refined)
}

fn square(corners: [P; 4], marker_mm: (f64, f64)) -> Result<Square> {
    let [tl, tr, br, bl] = corners;
    let width = dist(mid(tl, bl), mid(tr, br));
    let height = dist(mid(tl, tr), mid(bl, br));
    let px_per_mm = (width / marker_mm.0, height / marker_mm.1);
    let aspect = px_per_mm.0 / px_per_mm.1;
    if !(2.0 / 3.0..=1.5).contains(&aspect) {
        return Err(OpticalError::NoSquare);
    }
    let top = sub(tr, tl);
    let bottom = sub(br, bl);
    let centre = corners.iter().fold((0.0, 0.0), |acc, c| (acc.0 + c.0 / 4.0, acc.1 + c.1 / 4.0));
    Ok(Square {
        centre: PixelPoint { x: centre.0, y: centre.1 },
        corners: corners.map(|(x, y)| PixelPoint { x, y }),
        angle_deg: (-(top.1 + bottom.1)).atan2(top.0 + bottom.0).to_degrees(),
        px_per_mm,
    })
}

/// In `FindCentre` a square whose sides cannot be refined falls back to its rough rectangle, as Python's looser
/// 20-vertex simplification did; `Calibrate` refuses it.
pub fn find_square(frame: &Frame, params: &DetectParams) -> Result<Square> {
    let gray = frame.image()?;
    if frame.width < 2 || frame.height < 2 {
        return Err(OpticalError::NoSquare);
    }
    let coarse = rough(&gray, params)?;
    let refined = refine(frame, coarse).and_then(|first| refine(frame, first));
    match (refined, params.mode) {
        (Some(corners), _) => square(corners, params.marker_mm),
        (None, Mode::FindCentre) => square(coarse, params.marker_mm),
        (None, Mode::Calibrate) => Err(OpticalError::NoSquare),
    }
}
