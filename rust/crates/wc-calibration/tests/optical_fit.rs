//! The optical geometry fit against synthetic grids: a machine with known true geometry, driven by the owner's
//! configured one, puts each square where `wc-kinematics` says, and the fit must recover the difference.

use serde_json::Value;
use wc_calibration::optical_fit::{MAX_ERROR_MM, MIN_CELLS, PITCH_MM, fit};
use wc_calibration::{CalibrationError, MachineSettings, holey};
use wc_config::Settings;
use wc_kinematics::{Geometry, Kinematics};
use wc_protocol::optical::{Cell, GeometryFit};

type Errors = Vec<(Cell, (f64, f64))>;

fn owner() -> Settings {
    let path = format!("{}/tests/fixtures/optical_fit_owner.json", env!("CARGO_MANIFEST_DIR"));
    let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json");
    let mut settings = Settings::defaults();
    fixture["geometry"].as_object().expect("sections").iter().for_each(|(section, values)| {
        values.as_object().expect("values").iter().for_each(|(key, value)| {
            assert!(settings.set_value(section, key, value.clone()), "{section} {key}");
        });
    });
    settings
}

fn configured(settings: &Settings) -> Geometry {
    holey::model(&MachineSettings::from_lookup(|section, key| settings.text(section, key)).expect("settings"))
}

/// A true machine: motor spacing, motorOffsetY and chain tolerance deltas, then the sheet's x, y and rotation.
#[derive(Clone, Copy, Default)]
struct Truth {
    d: f64,
    offset: f64,
    left: f64,
    right: f64,
    sheet_x: f64,
    sheet_y: f64,
    rotation_deg: f64,
}

fn all_cells() -> Vec<Cell> {
    (-7..=7).flat_map(|j| (-15..=15).map(move |i| Cell { i, j })).collect()
}

/// Each square's error: the commanded position that lands the true sled on it, minus its nominal position.
fn synthetic(settings: &Settings, truth: Truth, cells: &[Cell]) -> Errors {
    let base = configured(settings);
    let mut configured = Kinematics::new(base.clone());
    let mut actual = Kinematics::new(Geometry {
        d: base.d + truth.d,
        motor_offset_y: base.motor_offset_y + truth.offset,
        left_chain_tolerance: base.left_chain_tolerance + truth.left,
        right_chain_tolerance: base.right_chain_tolerance + truth.right,
        ..base.clone()
    });
    let (sin, cos) = truth.rotation_deg.to_radians().sin_cos();
    cells
        .iter()
        .map(|&cell| {
            let (x, y) = (f64::from(cell.i) * PITCH_MM, f64::from(cell.j) * PITCH_MM);
            let target = (cos * x - sin * y + truth.sheet_x, sin * x + cos * y + truth.sheet_y);
            let (left, right) = actual.inverse_unclamped(target.0, target.1).expect("inverse");
            let (cx, cy) = configured.forward_from(left, right, (x, y)).expect("forward");
            (cell, (cx - x, cy - y))
        })
        .collect()
}

/// Gaussian noise of `sigma` mm from a seeded xorshift and Box-Muller.
fn noisy(errors: &Errors, sigma: f64, seed: u64) -> Errors {
    let mut state = seed;
    let mut uniform = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 11) as f64 / (1u64 << 53) as f64
    };
    let mut gauss = move || (-2.0 * (1.0 - uniform()).ln()).sqrt() * (std::f64::consts::TAU * uniform()).cos();
    errors.iter().map(|&(cell, (x, y))| (cell, (x + sigma * gauss(), y + sigma * gauss()))).collect()
}

/// The fitted values minus the true ones, tolerances as millimetres of a 1.8 m chain.
fn misfit(settings: &Settings, truth: Truth, fit: &GeometryFit) -> [f64; 4] {
    let base = configured(settings);
    let f = &fit.fitted;
    [
        f.distance_between_motors - (base.d + truth.d),
        f.motor_yoffset - (base.motor_offset_y + truth.offset),
        (f.left_chain_tolerance - (base.left_chain_tolerance + truth.left)) * 18.0,
        (f.right_chain_tolerance - (base.right_chain_tolerance + truth.right)) * 18.0,
    ]
}

fn assert_recovers(truth: Truth, cells: &[Cell], sigma: f64, tolerance: f64) -> GeometryFit {
    assert_recovers_seeded(truth, cells, sigma, 0x9e37_79b9_7f4a_7c15, tolerance)
}

fn assert_recovers_seeded(truth: Truth, cells: &[Cell], sigma: f64, seed: u64, tolerance: f64) -> GeometryFit {
    let settings = owner();
    let errors = noisy(&synthetic(&settings, truth, cells), sigma, seed);
    let fitted = fit(&settings, &errors).expect("fit");
    let misfit = misfit(&settings, truth, &fitted);
    assert!(misfit.iter().all(|m| m.abs() <= tolerance), "{misfit:?} beyond {tolerance} mm");
    assert_eq!(fitted.cells as usize, cells.len());
    fitted
}

#[test]
fn recovers_the_spec_s_motor_spacing_and_height_mismatch() {
    let truth = Truth { d: 3.0, offset: -5.0, ..Truth::default() };
    let fitted = assert_recovers(truth, &all_cells(), 0.0, 1e-3);
    assert!(fitted.rms_mm < 1e-4, "{}", fitted.rms_mm);
    let settings = owner();
    let base = configured(&settings);
    assert_eq!(fitted.current.distance_between_motors, base.d);
    assert_eq!(fitted.current.motor_yoffset, base.motor_offset_y);
    assert_eq!(fitted.current.left_chain_tolerance, base.left_chain_tolerance);
    assert_eq!(fitted.current.right_chain_tolerance, base.right_chain_tolerance);
}

#[test]
fn recovers_chain_tolerances_and_the_sheet_s_pose() {
    let truth = Truth { d: -2.0, offset: 4.0, left: 0.05, right: -0.03, sheet_x: 3.5, rotation_deg: 0.2, sheet_y: 0.0 };
    let fitted = assert_recovers(truth, &all_cells(), 0.0, 1e-3);
    assert!((fitted.sheet.x_mm - 3.5).abs() < 1e-3, "{:?}", fitted.sheet);
    assert!((fitted.sheet.rotation_deg - 0.2).abs() < 1e-4, "{:?}", fitted.sheet);
    assert_eq!(fitted.sheet.y_mm, 0.0);
}

#[test]
fn a_machine_that_matches_its_settings_fits_unchanged() {
    let fitted = assert_recovers(Truth::default(), &all_cells(), 0.0, 1e-4);
    assert_eq!(fitted.fitted, fitted.current);
    assert!(fitted.rms_mm < 1e-6, "{}", fitted.rms_mm);
}

#[test]
fn a_vertical_sheet_shift_reads_as_motor_height() {
    let settings = owner();
    let shifted = synthetic(&settings, Truth { sheet_y: 2.0, ..Truth::default() }, &all_cells());
    let fitted = fit(&settings, &shifted).expect("fit");
    assert!((fitted.fitted.motor_yoffset - fitted.current.motor_yoffset + 2.0).abs() < 1e-3, "{fitted:?}");
}

#[test]
fn detection_noise_moves_the_fit_less_than_half_a_millimetre() {
    let truth =
        Truth { d: 3.0, offset: -5.0, left: 0.02, right: 0.01, sheet_x: -1.0, rotation_deg: -0.1, sheet_y: 0.0 };
    (1..=8).for_each(|seed| {
        let fitted = assert_recovers_seeded(truth, &all_cells(), 0.05, seed, 0.5);
        assert!((0.03..0.1).contains(&fitted.rms_mm), "{}", fitted.rms_mm);
    });
}

#[test]
fn three_times_that_noise_still_fits_within_half_a_millimetre() {
    let truth = Truth { d: -4.0, offset: 6.0, left: -0.03, right: 0.04, sheet_x: 2.0, rotation_deg: 0.3, sheet_y: 0.0 };
    (1..=4).for_each(|seed| {
        assert_recovers_seeded(truth, &all_cells(), 0.15, seed, 0.5);
    });
}

#[test]
fn a_half_scan_with_noise_still_recovers() {
    let truth = Truth { d: 3.0, offset: -5.0, ..Truth::default() };
    let upper: Vec<Cell> = all_cells().into_iter().filter(|c| c.j >= 0).collect();
    assert_recovers(truth, &upper, 0.05, 0.5);
}

fn refusal(errors: &Errors) -> (CalibrationError, String) {
    let error = fit(&owner(), errors).expect_err("refused");
    let message = error.to_string();
    (error, message)
}

fn grid_refusal(errors: &Errors) -> String {
    match refusal(errors) {
        (CalibrationError::OpticalGrid(reason), message) if reason == message => message,
        other => panic!("{other:?}"),
    }
}

fn fit_refusal(errors: &Errors) -> String {
    match refusal(errors) {
        (CalibrationError::OpticalFit(reason), message) if reason == message => message,
        other => panic!("{other:?}"),
    }
}

fn exact(cells: &[Cell]) -> Errors {
    synthetic(&owner(), Truth { d: 3.0, offset: -5.0, ..Truth::default() }, cells)
}

#[test]
fn too_few_squares_are_refused() {
    let cells: Vec<Cell> = all_cells().into_iter().filter(|c| c.i.abs() <= 1 && c.j.abs() <= 1).collect();
    assert_eq!(cells.len(), MIN_CELLS);
    let mut errors = exact(&cells);
    errors.pop();
    assert_eq!(grid_refusal(&errors), "The geometry fit needs at least 9 measured squares; this scan has 8.");
}

#[test]
fn squares_along_one_line_are_refused() {
    let line = "The measured squares lie along one line; measure squares across at least three rows and three columns.";
    let row: Vec<Cell> = all_cells().into_iter().filter(|c| c.j == 2).collect();
    assert_eq!(grid_refusal(&exact(&row)), line);
    let diagonal: Vec<Cell> = (-7..=7).map(|k| Cell { i: k, j: k }).collect();
    assert_eq!(grid_refusal(&exact(&diagonal)), line);
}

#[test]
fn squares_off_the_grid_or_measured_twice_are_refused() {
    let mut errors = exact(&all_cells());
    errors[3].0 = Cell { i: 16, j: 0 };
    assert_eq!(grid_refusal(&errors), "Square (16, 0) is not on the target grid.");
    errors[3].0 = Cell { i: 0, j: -8 };
    assert_eq!(grid_refusal(&errors), "Square (0, -8) is not on the target grid.");
    errors[3].0 = errors[4].0;
    assert_eq!(grid_refusal(&errors), "Square (-11, -7) was measured twice.");
}

#[test]
fn an_impossible_error_is_refused_before_fitting() {
    let mut errors = exact(&all_cells());
    errors[40].1 = (MAX_ERROR_MM, 1.0);
    assert_eq!(
        grid_refusal(&errors),
        "Square (-6, -6) is 25.0 mm from where it should be, more than the 25 mm a scan allows; measure it again."
    );
    errors[40].1 = (f64::NAN, 0.0);
    assert_eq!(refusal(&errors).0, CalibrationError::NotFinite);
    errors[40].1 = (0.0, f64::INFINITY);
    assert_eq!(refusal(&errors).0, CalibrationError::NotFinite);
}

#[test]
fn an_outlying_square_is_named() {
    let mut errors = noisy(&exact(&all_cells()), 0.05, 7);
    errors[100].1.0 += 3.0;
    let message = fit_refusal(&errors);
    assert!(message.starts_with("Square (-8, -4) is 2."), "{message}");
    assert!(
        message
            .ends_with("mm off the fitted geometry, where most squares agree to 0.06 mm; measure it again or skip it."),
        "{message}"
    );
}

#[test]
fn a_fit_that_explains_little_is_refused() {
    let errors = noisy(&exact(&all_cells()), 1.0, 11);
    let message = fit_refusal(&errors);
    assert!(message.starts_with("The fitted geometry still leaves 1."), "{message}");
    assert!(
        message.ends_with("mm RMS error, more than 1 mm; check the target sheet is flat and pinned, then scan again."),
        "{message}"
    );
}

#[test]
fn a_small_patch_cannot_pin_the_geometry_down() {
    let patch: Vec<Cell> = all_cells().into_iter().filter(|c| c.i.abs() <= 1 && c.j.abs() <= 1).collect();
    let message = fit_refusal(&noisy(&exact(&patch), 0.05, 3));
    assert!(message.starts_with("The measured squares do not pin down the "), "{message}");
    assert!(message.ends_with(" to within 0.5 mm; measure squares across more of the sheet."), "{message}");
}

#[test]
fn settings_that_are_not_numbers_are_refused() {
    let mut settings = owner();
    settings.set_value("Maslow Settings", "motorSpacingX", Value::from("wide"));
    let error = fit(&settings, &exact(&all_cells())).expect_err("refused");
    assert_eq!(error.to_string(), "Please enter a number for the setting motorSpacingX in Maslow Settings.");
}
