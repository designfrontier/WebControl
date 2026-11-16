//! Track F: the holey geometry (motor spacing, motorOffsetY, chain tolerances) and the target sheet's pose that best
//! explain an optical error grid, solved with `trf.rs`.

use std::collections::HashSet;

use nalgebra::{Matrix2, Vector2};
use wc_config::Settings;
use wc_kinematics::{Geometry, Kinematics};
use wc_protocol::calibration::HoleyParameters;
use wc_protocol::optical::{Cell, GeometryFit, SheetPose};

use crate::trf::{self, Vector};
use crate::{CalibrationError, MachineSettings, holey, python};

pub const PITCH_MM: f64 = 76.2;
const HALF_COLUMNS: i32 = 15;
const HALF_ROWS: i32 = 7;
pub const MIN_CELLS: usize = 9;
/// A scan fails a cell whose correction passes 25 mm, so a larger error is not a measurement.
pub const MAX_ERROR_MM: f64 = 25.0;
/// The narrowest spread of the squares across their least-spread direction, as a standard deviation.
const MIN_SPREAD_MM: f64 = 0.4 * PITCH_MM;
const OUTLIER_FLOOR_MM: f64 = 1.0;
const OUTLIER_MEDIANS: f64 = 5.0;
const MAX_RMS_MM: f64 = 1.0;
const MAX_STD_ERROR_MM: f64 = 0.5;
/// Detection noise the precision check assumes even when the residuals are smaller.
const NOISE_FLOOR_MM: f64 = 0.05;
const JACOBIAN_STEP_MM: f64 = 0.01;
/// Fitted: motor spacing, motorOffsetY, left and right chain tolerance, sheet x and sheet rotation in mrad.
const PARAMETERS: usize = 6;
const NAMES: [&str; 4] =
    ["the motor spacing", "the motor height (motorOffsetY)", "the left chain tolerance", "the right chain tolerance"];

/// One measured square: where the sheet puts it, the chains the machine drove to, and chain to sled mm.
struct Point {
    cell: Cell,
    nominal: Vector2<f64>,
    chains: Vector2<f64>,
    to_mm: Matrix2<f64>,
}

/// `errors` holds each measured square's (x, y) error in mm, as `wc_optical::grid::ErrorGrid::measured` lists them.
/// A vertical sheet shift moves every square exactly as motorOffsetY does, so the sheet's centre is held level with
/// the work area's and only its x offset and rotation are fitted.
pub fn fit(settings: &Settings, errors: &[(Cell, (f64, f64))]) -> Result<GeometryFit, CalibrationError> {
    let machine = MachineSettings::from_lookup(|section, key| settings.text(section, key))
        .map_err(|e| CalibrationError::NotANumber(format!("the setting {} in {}", e.key, e.section)))?;
    check_grid(errors)?;
    let base = holey::model(&machine);
    let points = errors.iter().map(|&(cell, error)| point(&base, cell, error)).collect::<Result<Vec<_>, _>>()?;
    let fitted = solve(&base, &points).ok_or(CalibrationError::NoSolution)?;
    let p = fitted.x;
    let rms_mm = check_quality(&base, &points, &p, &fitted.f)?;
    let current = parameters(&base, &Vector::zeros(PARAMETERS));
    let fitted = parameters(&base, &p);
    let sheet = SheetPose { x_mm: p[4], y_mm: 0.0, rotation_deg: (p[5] / 1000.0).to_degrees() };
    Ok(GeometryFit { current, fitted, sheet, rms_mm, cells: points.len() as u32 })
}

/// Where a machine built to `built`'s geometry puts the sled when its firmware drives to `commanded` with
/// `configured`'s; `None` for settings that are not numbers or chains that cannot reach.
pub fn misplaced(configured: &Settings, built: &Settings, commanded: (f64, f64)) -> Option<(f64, f64)> {
    let model = |settings: &Settings| {
        MachineSettings::from_lookup(|section, key| settings.text(section, key)).ok().map(|m| holey::model(&m))
    };
    let (left, right) = Kinematics::new(model(configured)?).inverse_unclamped(commanded.0, commanded.1)?;
    Kinematics::new(model(built)?).forward_from(left, right, commanded)
}

fn solve(base: &Geometry, points: &[Point]) -> Option<trf::Fitted> {
    trf::solve(|p: &Vector| residuals(base, points, p), Vector::zeros(PARAMETERS))
        .filter(|fitted| fitted.x.iter().chain(fitted.f.iter()).all(|v| v.is_finite()))
}

fn at(cell: Cell) -> String {
    format!("Square ({}, {})", cell.i, cell.j)
}

fn check_grid(errors: &[(Cell, (f64, f64))]) -> Result<(), CalibrationError> {
    let refuse = |reason: String| Err(CalibrationError::OpticalGrid(reason));
    if errors.len() < MIN_CELLS {
        return refuse(format!(
            "The geometry fit needs at least {MIN_CELLS} measured squares; this scan has {}.",
            errors.len()
        ));
    }
    let mut seen = HashSet::new();
    for &(cell, (x, y)) in errors {
        if cell.i.abs() > HALF_COLUMNS || cell.j.abs() > HALF_ROWS {
            return refuse(format!("{} is not on the target grid.", at(cell)));
        }
        if !seen.insert(cell) {
            return refuse(format!("{} was measured twice.", at(cell)));
        }
        if !(x.is_finite() && y.is_finite()) {
            return Err(CalibrationError::NotFinite);
        }
        let error = x.hypot(y);
        if error > MAX_ERROR_MM {
            return refuse(format!(
                "{} is {error:.1} mm from where it should be, more than the {MAX_ERROR_MM} mm a scan allows; measure it again.",
                at(cell)
            ));
        }
    }
    if narrowest_spread(errors) < MIN_SPREAD_MM {
        return refuse(
            "The measured squares lie along one line; measure squares across at least three rows and three columns."
                .into(),
        );
    }
    Ok(())
}

/// The standard deviation of the squares' positions along the direction they spread least.
fn narrowest_spread(errors: &[(Cell, (f64, f64))]) -> f64 {
    let n = errors.len() as f64;
    let positions: Vec<Vector2<f64>> = errors.iter().map(|(cell, _)| nominal(*cell)).collect();
    let mean = positions.iter().sum::<Vector2<f64>>() / n;
    let covariance = positions.iter().map(|p| (p - mean) * (p - mean).transpose()).sum::<Matrix2<f64>>() / n;
    covariance.symmetric_eigenvalues().min().max(0.0).sqrt()
}

fn nominal(cell: Cell) -> Vector2<f64> {
    Vector2::new(f64::from(cell.i) * PITCH_MM, f64::from(cell.j) * PITCH_MM)
}

fn inverse(kinematics: &mut Kinematics, at: Vector2<f64>) -> Option<Vector2<f64>> {
    kinematics.inverse_unclamped(at.x, at.y).map(|(left, right)| Vector2::new(left, right))
}

/// The firmware clamps the commanded target, then drives the configured model's chains; `to_mm` inverts that
/// model's chain Jacobian there, so a chain mismatch reads as sled millimetres.
fn point(base: &Geometry, cell: Cell, (x, y): (f64, f64)) -> Result<Point, CalibrationError> {
    let mut kinematics = Kinematics::new(base.clone());
    let nominal = nominal(cell);
    let unreachable = || CalibrationError::OpticalGrid(format!("{} is out of the machine's reach.", at(cell)));
    let (cx, cy) = base.clamp(nominal.x + x, nominal.y + y);
    let commanded = Vector2::new(cx, cy);
    let chains = inverse(&mut kinematics, commanded).ok_or_else(unreachable)?;
    let h = JACOBIAN_STEP_MM;
    let mut column = |step: Vector2<f64>| -> Option<Vector2<f64>> {
        Some((inverse(&mut kinematics, commanded + step)? - inverse(&mut kinematics, commanded - step)?) / (2.0 * h))
    };
    let jacobian = Matrix2::from_columns(&[
        column(Vector2::new(h, 0.0)).ok_or_else(unreachable)?,
        column(Vector2::new(0.0, h)).ok_or_else(unreachable)?,
    ]);
    let to_mm = jacobian.try_inverse().ok_or_else(unreachable)?;
    Ok(Point { cell, nominal, chains, to_mm })
}

fn geometry(base: &Geometry, p: &Vector) -> Geometry {
    Geometry {
        d: base.d + p[0],
        motor_offset_y: base.motor_offset_y + p[1],
        left_chain_tolerance: base.left_chain_tolerance + p[2],
        right_chain_tolerance: base.right_chain_tolerance + p[3],
        ..base.clone()
    }
}

/// Each square's predicted sled position minus where the sheet puts it, x then y, in mm.
fn residuals(base: &Geometry, points: &[Point], p: &Vector) -> Option<Vector> {
    let mut kinematics = Kinematics::new(geometry(base, p));
    let rotation = nalgebra::Rotation2::new(p[5] / 1000.0);
    let offset = Vector2::new(p[4], 0.0);
    let mut out = Vector::zeros(2 * points.len());
    points.iter().enumerate().try_for_each(|(k, point)| {
        let target = rotation * point.nominal + offset;
        let mismatch = point.to_mm * (point.chains - inverse(&mut kinematics, target)?);
        out.fixed_rows_mut::<2>(2 * k).copy_from(&mismatch);
        Some(())
    })?;
    Some(out)
}

fn parameters(base: &Geometry, p: &Vector) -> HoleyParameters {
    let g = geometry(base, p);
    HoleyParameters {
        motor_yoffset: python::round(g.motor_offset_y, 5),
        distance_between_motors: python::round(g.d, 5),
        left_chain_tolerance: python::round(g.left_chain_tolerance, 5),
        right_chain_tolerance: python::round(g.right_chain_tolerance, 5),
    }
}

/// Refuses an outlying square, a poor fit or an imprecise one; answers the RMS residual in mm.
fn check_quality(base: &Geometry, points: &[Point], p: &Vector, f: &Vector) -> Result<f64, CalibrationError> {
    let refuse = |reason: String| Err(CalibrationError::OpticalFit(reason));
    let lengths: Vec<f64> = (0..points.len()).map(|k| f[2 * k].hypot(f[2 * k + 1])).collect();
    let rms = (f.norm_squared() / points.len() as f64).sqrt();
    let mut sorted = lengths.clone();
    sorted.sort_by(f64::total_cmp);
    let limit = (OUTLIER_MEDIANS * sorted[sorted.len() / 2]).max(OUTLIER_FLOOR_MM);
    let (worst, largest) = lengths.iter().enumerate().fold((0, 0.0), |a, (k, &l)| if l > a.1 { (k, l) } else { a });
    if largest > limit {
        return refuse(format!(
            "{} is {largest:.2} mm off the fitted geometry, where most squares agree to {:.2} mm; measure it again or skip it.",
            at(points[worst].cell),
            sorted[sorted.len() / 2]
        ));
    }
    if rms > MAX_RMS_MM {
        return refuse(format!(
            "The fitted geometry still leaves {rms:.2} mm RMS error, more than {MAX_RMS_MM} mm; check the target sheet is flat and pinned, then scan again."
        ));
    }
    let degrees_of_freedom = (f.len() - PARAMETERS) as f64;
    let sigma = (f.norm_squared() / degrees_of_freedom).sqrt().max(NOISE_FLOOR_MM);
    let errors = standard_errors(base, points, p, sigma, f);
    let imprecise = errors.iter().position(|e| e.is_nan() || *e > MAX_STD_ERROR_MM);
    match imprecise {
        Some(k) => refuse(format!(
            "The measured squares do not pin down {} to within {MAX_STD_ERROR_MM} mm; measure squares across more of the sheet.",
            NAMES[k]
        )),
        None => Ok(rms),
    }
}

/// One standard error per geometry value in mm, a chain tolerance's as the chain length it changes mid-sheet.
fn standard_errors(base: &Geometry, points: &[Point], p: &Vector, sigma: f64, f: &Vector) -> [f64; 4] {
    let h = JACOBIAN_STEP_MM;
    let columns: Vec<Vector> = (0..PARAMETERS)
        .map(|i| {
            let mut stepped = p.clone();
            stepped[i] += h;
            residuals(base, points, &stepped).map_or_else(|| Vector::from_element(f.len(), f64::NAN), |r| (r - f) / h)
        })
        .collect();
    let jacobian = nalgebra::DMatrix::from_columns(&columns);
    let covariance = (jacobian.transpose() * jacobian).try_inverse();
    let chain = Kinematics::new(geometry(base, p)).inverse(0.0, 0.0).map_or(f64::NAN, |(left, _)| left);
    std::array::from_fn(|k| {
        let scale = if k < 2 { 1.0 } else { chain / 100.0 };
        covariance.as_ref().map_or(f64::INFINITY, |c| sigma * c[(k, k)].max(0.0).sqrt() * scale)
    })
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn read(path: &str) -> Value {
        let path = format!("{}/{path}", env!("CARGO_MANIFEST_DIR"));
        serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json")
    }

    fn owner() -> Geometry {
        let fixture = read("tests/fixtures/optical_fit_owner.json");
        let geometry = &fixture["geometry"];
        let text = |value: &Value| value.as_str().map_or_else(|| value.to_string(), str::to_string);
        holey::model(&MachineSettings::from_lookup(|section, key| geometry[section].get(key).map(text)).unwrap())
    }

    /// The case `optical_fit_path.json` pins: every square of a machine whose true geometry is off by `truth`.
    fn case(base: &Geometry, truth: &Vector) -> Vec<Point> {
        let mut actual = Kinematics::new(geometry(base, truth));
        let mut configured = Kinematics::new(base.clone());
        let rotation = nalgebra::Rotation2::new(truth[5] / 1000.0);
        let cells = (-HALF_ROWS..=HALF_ROWS).flat_map(|j| (-HALF_COLUMNS..=HALF_COLUMNS).map(move |i| Cell { i, j }));
        cells
            .map(|cell| {
                let target = rotation * nominal(cell) + Vector2::new(truth[4], 0.0);
                let (left, right) = actual.inverse_unclamped(target.x, target.y).unwrap();
                let (x, y) = configured.forward_from(left, right, (target.x, target.y)).unwrap();
                point(base, cell, (x - nominal(cell).x, y - nominal(cell).y)).unwrap()
            })
            .collect()
    }

    fn close(actual: f64, expected: &Value, relative: f64, floor: f64) -> bool {
        let expected = expected.as_f64().expect("number");
        (actual - expected).abs() <= relative * expected.abs() + floor
    }

    /// Recorded from this fit, since Python had none: a changed path means a changed model or solver. Regenerate
    /// with `OPTICAL_FIT_REGENERATE=1 cargo test -p wc-calibration --lib optical_fit`.
    #[test]
    fn the_fit_takes_its_pinned_path() {
        let base = owner();
        let truth = Vector::from_vec(vec![3.0, -5.0, 0.02, -0.01, 1.5, 1.75]);
        let fitted = solve(&base, &case(&base, &truth)).expect("fit");
        let rows: Vec<Value> = fitted
            .iterations
            .iter()
            .map(|i| json!({"nfev": i.nfev, "cost": i.cost, "reduction": i.reduction, "stepNorm": i.step_norm, "optimality": i.optimality}))
            .collect();
        let path = "tests/fixtures/optical_fit_path.json";
        if std::env::var_os("OPTICAL_FIT_REGENERATE").is_some() {
            let pinned = json!({"truth": truth.as_slice(), "fitted": fitted.x.as_slice(), "iterations": rows});
            let file = format!("{}/{path}", env!("CARGO_MANIFEST_DIR"));
            std::fs::write(file, serde_json::to_string_pretty(&pinned).unwrap() + "\n").unwrap();
        }
        let pinned = read(path);
        assert_eq!(pinned["truth"], json!(truth.as_slice()));
        let expected = pinned["iterations"].as_array().expect("iterations");
        assert_eq!(rows.len(), expected.len());
        let noise = 1e-6 * expected[0]["cost"].as_f64().unwrap() + 1e-12;
        rows.iter().zip(expected).enumerate().for_each(|(k, (actual, expected))| {
            assert_eq!(actual["nfev"], expected["nfev"], "iteration {k}");
            ["cost", "reduction", "stepNorm", "optimality"].iter().for_each(|field| match actual[field].as_f64() {
                None => assert!(expected[field].is_null(), "iteration {k}: {field}"),
                // Linux x86 and macOS arm differ by ~3e-4 here from rounding; a model change moves it far more.
                Some(value) => assert!(close(value, &expected[field], 1e-3, noise), "iteration {k}: {field} {value}"),
            });
        });
        fitted.x.iter().zip(pinned["fitted"].as_array().unwrap()).for_each(|(x, e)| assert!(close(*x, e, 1e-4, 1e-6)));
        assert!((fitted.x - truth).amax() < 1e-4);
    }
}
