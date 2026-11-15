//! `Actions/holeyCalibration.py`: six holes cut with the configured machine, twelve distances measured between
//! them, and a least-squares fit of the motor spacing, motor height and chain tolerances that explains them.

use levenberg_marquardt::{LeastSquaresProblem, LevenbergMarquardt, TerminationReason};
use nalgebra::{Owned, SMatrix, SVector, U4, U12};
use serde_json::Value;
use wc_config::python_float_str;
use wc_kinematics::{Geometry, Kinematics, Mode, sprocket_radius};

use crate::accept::{Accept, SettingWrite};
use crate::{CalibrationError, MachineSettings, python};

pub const MEASUREMENTS: usize = 12;

/// The holes each of M1 to M11 spans, numbered 1 to 6 as on the pattern. M12 runs from hole 2 to the top edge.
const MEASUREMENT_MAP: [(usize, usize); 11] =
    [(1, 2), (2, 3), (4, 5), (5, 6), (1, 4), (2, 5), (3, 6), (2, 4), (1, 5), (3, 5), (2, 6)];
/// Counter-clockwise from the top middle hole.
const CUT_ORDER: [usize; 6] = [1, 0, 3, 4, 5, 2];
/// scipy's `diff_step`: each finite-difference step is a tenth of the parameter, or 0.1 near zero.
const DIFF_STEP: f64 = 0.1;

type Params = SVector<f64, 4>;
type Residuals = SVector<f64, MEASUREMENTS>;

/// The fitted machine, rounded to 5 places as Python rounds it, and the root of the summed squared residuals.
#[derive(Debug, Clone, PartialEq)]
pub struct HoleyResult {
    pub motor_y_offset: f64,
    pub distance_between_motors: f64,
    pub left_chain_tolerance: f64,
    pub right_chain_tolerance: f64,
    pub total_error: f64,
}

/// Hole centres for the work area, holes 1 to 3 along the top and 4 to 6 along the bottom, 254 mm from the edges.
pub fn ideal_coordinates(settings: &MachineSettings) -> [(f64, f64); 6] {
    let x = -(settings.bed_width / 2.0 - 254.0);
    let y = settings.bed_height / 2.0 - 254.0;
    [(x, y), (0.0, y), (-x, y), (x, -y), (0.0, -y), (-x, -y)]
}

/// The kinematics the fit uses: the firmware's triangular model with the configured machine.
pub fn model(settings: &MachineSettings) -> Geometry {
    let defaults = Geometry::default();
    Geometry {
        d: settings.motor_spacing_x,
        r: sprocket_radius(settings.dist_per_rot()),
        machine_width: settings.bed_width,
        machine_height: settings.bed_height,
        motor_offset_y: settings.motor_offset_y,
        rotation_disk_radius: settings.rotation_radius,
        sled_weight: settings.sled_weight,
        chain_elasticity: settings.chain_elasticity.unwrap_or(defaults.chain_elasticity),
        left_chain_tolerance: settings.left_chain_tolerance,
        right_chain_tolerance: settings.right_chain_tolerance,
        chain_over_sprocket: settings.chain_over_sprocket,
        mode: Mode::Triangular,
        ..defaults
    }
}

fn length((x1, y1): (f64, f64), (x2, y2): (f64, f64)) -> f64 {
    ((x1 - x2) * (x1 - x2) + (y1 - y2) * (y1 - y2)).sqrt()
}

/// `CalculateMeasurements`: M1 to M12 for these hole positions.
fn measurements_of(holes: &[(f64, f64); 6], machine_height: f64) -> [f64; MEASUREMENTS] {
    let top = holes[1];
    let mut lengths = [0.0; MEASUREMENTS];
    MEASUREMENT_MAP.iter().enumerate().for_each(|(i, (a, b))| lengths[i] = length(holes[a - 1], holes[b - 1]));
    lengths[MEASUREMENTS - 1] = length(top, (top.0, machine_height / 2.0));
    lengths
}

/// What M1 to M12 should read on a machine that matches its settings.
pub fn ideal_lengths(settings: &MachineSettings) -> [f64; MEASUREMENTS] {
    measurements_of(&ideal_coordinates(settings), settings.bed_height)
}

/// The chain lengths the firmware drives to for each hole, with the configured machine.
fn chain_lengths(settings: &MachineSettings) -> [(f64, f64); 6] {
    let mut kinematics = Kinematics::new(model(settings));
    ideal_coordinates(settings).map(|(x, y)| kinematics.inverse(x, y).unwrap_or((f64::NAN, f64::NAN)))
}

/// Where a machine with these parameters puts each hole, or `None` if it cannot reach one.
fn holes_for(base: &Geometry, chains: &[(f64, f64); 6], params: &Params) -> Option<[(f64, f64); 6]> {
    let mut kinematics = Kinematics::new(Geometry {
        d: base.d + params[0],
        motor_offset_y: base.motor_offset_y + params[1],
        left_chain_tolerance: base.left_chain_tolerance + params[2],
        right_chain_tolerance: base.right_chain_tolerance + params[3],
        ..base.clone()
    });
    let mut holes = [(0.0, 0.0); 6];
    chains.iter().zip(holes.iter_mut()).try_for_each(|((left, right), hole)| {
        *hole = kinematics.forward(*left, *right)?;
        Some(())
    })?;
    Some(holes)
}

/// `SimulateMeasurement`: what M1 to M12 read on a machine with these true parameters, cut with the configured one.
pub fn simulate_measurements(
    settings: &MachineSettings,
    d: f64,
    motor_offset_y: f64,
    left_chain_tolerance: f64,
    right_chain_tolerance: f64,
) -> Option<[f64; MEASUREMENTS]> {
    let truth = Geometry { d, motor_offset_y, left_chain_tolerance, right_chain_tolerance, ..model(settings) };
    let holes = holes_for(&truth, &chain_lengths(settings), &Params::zeros())?;
    Some(measurements_of(&holes, truth.machine_height))
}

/// `processMeasurements`: twelve numbers, or the first field Python's `float()` refuses.
pub fn parse_measurements(values: &[Value]) -> Result<[f64; MEASUREMENTS], CalibrationError> {
    let mut parsed = [0.0; MEASUREMENTS];
    (0..MEASUREMENTS).try_for_each(|i| {
        parsed[i] = values
            .get(i)
            .and_then(python::float)
            .ok_or(CalibrationError::NotANumber(format!("the distance M{}", i + 1)))?;
        Ok(())
    })?;
    Ok(parsed)
}

struct Fit {
    base: Geometry,
    chains: [(f64, f64); 6],
    measured: Residuals,
    params: Params,
}

impl Fit {
    /// `LengthDeltaFromIdeal`: measured minus calculated, for each of M1 to M12.
    fn residuals_at(&self, params: &Params) -> Option<Residuals> {
        let holes = holes_for(&self.base, &self.chains, params)?;
        Some(self.measured - Residuals::from(measurements_of(&holes, self.base.machine_height)))
    }
}

impl LeastSquaresProblem<f64, U12, U4> for Fit {
    type ResidualStorage = Owned<f64, U12>;
    type JacobianStorage = Owned<f64, U12, U4>;
    type ParameterStorage = Owned<f64, U4>;

    fn set_params(&mut self, params: &Params) {
        self.params = *params;
    }

    fn params(&self) -> Params {
        self.params
    }

    fn residuals(&self) -> Option<Residuals> {
        self.residuals_at(&self.params)
    }

    /// scipy's `2-point` Jacobian with `diff_step=0.1`: forward differences with a step relative to each parameter.
    fn jacobian(&self) -> Option<SMatrix<f64, MEASUREMENTS, 4>> {
        let f0 = self.residuals()?;
        let mut jacobian = SMatrix::<f64, MEASUREMENTS, 4>::zeros();
        (0..4).try_for_each(|j| {
            let x = self.params[j];
            let sign = if x >= 0.0 { 1.0 } else { -1.0 };
            let mut stepped = self.params;
            stepped[j] = x + DIFF_STEP * sign * x.abs().max(1.0);
            let step = stepped[j] - x;
            jacobian.set_column(j, &((self.residuals_at(&stepped)? - f0) / step));
            Some(())
        })?;
        Some(jacobian)
    }
}

/// `Calibrate`: fits the machine to the twelve measurements from the form.
pub fn calibrate(settings: &MachineSettings, values: &[Value]) -> Result<HoleyResult, CalibrationError> {
    calibrate_measurements(settings, &parse_measurements(values)?)
}

pub fn calibrate_measurements(
    settings: &MachineSettings,
    measured: &[f64; MEASUREMENTS],
) -> Result<HoleyResult, CalibrationError> {
    let base = model(settings);
    let fit =
        Fit { base, chains: chain_lengths(settings), measured: Residuals::from(*measured), params: Params::zeros() };
    let (fit, report) = LevenbergMarquardt::new().with_ftol(1e-11).minimize(fit);
    let failed = matches!(report.termination, TerminationReason::User(_) | TerminationReason::Numerical(_));
    let deltas = fit.params;
    if failed || !deltas.iter().all(|d| d.is_finite()) {
        return Err(CalibrationError::NoSolution);
    }
    let residuals = fit.residuals().ok_or(CalibrationError::NoSolution)?;
    Ok(HoleyResult {
        distance_between_motors: python::round(deltas[0] + fit.base.d, 5),
        motor_y_offset: python::round(deltas[1] + fit.base.motor_offset_y, 5),
        left_chain_tolerance: python::round(deltas[2] + fit.base.left_chain_tolerance, 5),
        right_chain_tolerance: python::round(deltas[3] + fit.base.right_chain_tolerance, 5),
        total_error: residuals.norm(),
    })
}

impl HoleyResult {
    /// `acceptCalibrationResults`: saves the fit; the firmware settings follow from the changed values.
    pub fn accept(&self) -> Accept {
        let write = |section, key, value: f64| SettingWrite { section, key, value: python_float_str(value) };
        Accept {
            writes: vec![
                write("Maslow Settings", "motorOffsetY", self.motor_y_offset),
                write("Maslow Settings", "motorSpacingX", self.distance_between_motors),
                write("Advanced Settings", "leftChainTolerance", self.left_chain_tolerance),
                write("Advanced Settings", "rightChainTolerance", self.right_chain_tolerance),
            ],
            gcode: Vec::new(),
        }
    }
}

/// `CutTestPattern`: the six holes, raising the bit before each move and plunging at a feed rate.
pub fn test_pattern(settings: &MachineSettings, inches: bool) -> Vec<String> {
    let holes = ideal_coordinates(settings);
    // Python's middle column is the integer 0, so it prints without a decimal point.
    let coordinate = |value: f64, middle: bool| if middle { "0".to_string() } else { python_float_str(value) };
    let cuts = CUT_ORDER.iter().flat_map(|&index| {
        let (x, y) = holes[index];
        let middle = index == 1 || index == 4;
        [format!("G0 X{} Y{}", coordinate(x, middle), python_float_str(y)), "G1 Z-5 F500".into(), "G0 Z5".into()]
    });
    ["G21", "G90", "G40", "G17", "G0 Z5", "M3"]
        .into_iter()
        .map(String::from)
        .chain(cuts)
        .chain(["G0 X0 Y0".to_string(), "M5".to_string()])
        .chain(inches.then(|| "G20".to_string()))
        .collect()
}
