//! Ported from the Python backend's test_holey_calibration_model, test_holey_test_pattern and the pure half of
//! test_calibration_accept, plus the refusals Python reports on its message queue.
#![allow(clippy::approx_constant)]

use std::collections::HashMap;

use serde_json::{Value, json};
use wc_calibration::{CalibrationError, MachineSettings, SettingWrite, holey, triangular};
use wc_kinematics::{CHAIN_DENSITY, ChainFeed, Geometry};

const DEFAULTS: [(&str, &str, &str); 14] = [
    ("Maslow Settings", "bedWidth", "2438.4"),
    ("Maslow Settings", "bedHeight", "1219.2"),
    ("Maslow Settings", "motorSpacingX", "2978.4"),
    ("Maslow Settings", "motorOffsetY", "463"),
    ("Maslow Settings", "sledWeight", "97.9"),
    ("Advanced Settings", "rotationRadius", "139.1"),
    ("Advanced Settings", "chainSagCorrection", "33"),
    ("Advanced Settings", "leftChainTolerance", "0"),
    ("Advanced Settings", "rightChainTolerance", "0"),
    ("Advanced Settings", "chainOverSprocket", "Bottom"),
    ("Advanced Settings", "gearTeeth", "10"),
    ("Advanced Settings", "chainPitch", "6.35"),
    ("Advanced Settings", "chainElasticity", "5.1685e-6"),
    ("Advanced Settings", "kinematicsType", "Triangular"),
];

/// test_holey_calibration_model's USER_SETTINGS: a machine someone calibrated before.
const USER: [(&str, &str, &str); 6] = [
    ("Maslow Settings", "motorSpacingX", "3011.16237"),
    ("Maslow Settings", "motorOffsetY", "433.68186"),
    ("Advanced Settings", "leftChainTolerance", "-0.06495"),
    ("Advanced Settings", "rightChainTolerance", "-0.02111"),
    ("Advanced Settings", "chainElasticity", "5.1685e-06"),
    ("Advanced Settings", "rotationRadius", "138.4"),
];

type Store = HashMap<(String, String), String>;

fn store(overrides: &[(&str, &str, &str)]) -> Store {
    DEFAULTS
        .iter()
        .chain(overrides)
        .map(|(section, key, value)| ((section.to_string(), key.to_string()), value.to_string()))
        .collect()
}

fn without(mut store: Store, section: &str, key: &str) -> Store {
    store.remove(&(section.to_string(), key.to_string()));
    store
}

fn lookup(store: &Store) -> impl Fn(&str, &str) -> Option<String> + '_ {
    |section, key| store.get(&(section.to_string(), key.to_string())).cloned()
}

fn settings_from(store: &Store) -> MachineSettings {
    MachineSettings::from_lookup(lookup(store)).expect("settings")
}

fn settings(overrides: &[(&str, &str, &str)]) -> MachineSettings {
    settings_from(&store(overrides))
}

fn form(values: &[f64]) -> Vec<Value> {
    values.iter().map(|value| json!(value)).collect()
}

#[test]
fn the_settings_are_read_from_their_stored_strings() {
    let read = settings(&[]);
    assert_eq!(read.bed_width, 2438.4);
    assert_eq!(read.motor_offset_y, 463.0);
    assert_eq!(read.chain_over_sprocket, ChainFeed::Bottom);
    assert_eq!(read.chain_elasticity, Some(5.1685e-6));
    assert_eq!(read.dist_per_rot(), 10.0 * 6.35);
    assert_eq!(settings(&[("Advanced Settings", "chainOverSprocket", "Top")]).chain_over_sprocket, ChainFeed::Top);
}

#[test]
fn a_setting_that_is_missing_or_not_a_number_is_refused() {
    let missing = MachineSettings::from_lookup(lookup(&without(store(&[]), "Maslow Settings", "bedWidth")));
    let error = missing.expect_err("refused");
    assert_eq!((error.section.as_str(), error.key.as_str()), ("Maslow Settings", "bedWidth"));
    assert_eq!(error.to_string(), "The setting bedWidth in Maslow Settings is not a number.");
    let garbage = store(&[("Advanced Settings", "chainElasticity", "stretchy")]);
    assert_eq!(MachineSettings::from_lookup(lookup(&garbage)).expect_err("refused").key, "chainElasticity");
}

#[test]
fn the_fit_uses_the_firmware_sprocket_radius_and_chain_density() {
    let model = holey::model(&settings(&USER));
    assert_eq!(model.r, 63.5 / (2.0 * 3.14159));
    assert_eq!(model.chain_density, CHAIN_DENSITY);
}

#[test]
fn the_sprocket_radius_follows_the_gear_and_chain_settings() {
    let model = holey::model(&settings(&[("Advanced Settings", "gearTeeth", "12")]));
    assert_eq!(model.r, 12.0 * 6.35 / (2.0 * 3.14159));
}

#[test]
fn the_fit_uses_the_chain_elasticity_setting_or_the_model_default() {
    assert_eq!(
        holey::model(&settings(&[("Advanced Settings", "chainElasticity", "6.2e-06")])).chain_elasticity,
        6.2e-06
    );
    let absent = settings_from(&without(store(&[]), "Advanced Settings", "chainElasticity"));
    assert_eq!(absent.chain_elasticity, None);
    assert_eq!(holey::model(&absent).chain_elasticity, Geometry::default().chain_elasticity);
}

#[test]
fn the_sled_geometry_keeps_the_model_defaults() {
    let model = holey::model(&settings(&[("Maslow Settings", "sledWidth", "0")]));
    let defaults = Geometry::default();
    assert_eq!((model.l, model.s, model.h3), (defaults.l, defaults.s, defaults.h3));
}

#[test]
fn the_fit_recovers_a_machine_the_firmware_drives() {
    let configured = settings(&USER);
    let measured = holey::simulate_measurements(&configured, 3022.76237, 440.68186, 0.23505, -0.22111).expect("holes");
    let result = holey::calibrate(&configured, &form(&measured)).expect("fit");
    assert!((result.distance_between_motors - 3022.76237).abs() <= 2e-4, "{result:?}");
    assert!((result.motor_y_offset - 440.68186).abs() <= 2e-4, "{result:?}");
    assert!((result.left_chain_tolerance - 0.23505).abs() <= 2e-5, "{result:?}");
    assert!((result.right_chain_tolerance + 0.22111).abs() <= 2e-5, "{result:?}");
}

#[test]
fn ideal_measurements_give_back_the_configured_machine() {
    let configured = settings(&USER);
    let result = holey::calibrate_measurements(&configured, &holey::ideal_lengths(&configured)).expect("fit");
    assert_eq!(result.distance_between_motors, 3011.16237);
    assert_eq!(result.motor_y_offset, 433.68186);
    assert!(result.total_error < 1e-6);
}

#[test]
fn a_blank_or_missing_measurement_is_refused_by_name() {
    let mut measurements = form(&holey::ideal_lengths(&settings(&[])));
    measurements[4] = json!("");
    let refused = holey::calibrate(&settings(&[]), &measurements).expect_err("refused");
    assert_eq!(refused, CalibrationError::NotANumber("the distance M5".into()));
    assert_eq!(refused.to_string(), "Please enter a number for the distance M5.");
    let short = holey::calibrate(&settings(&[]), &measurements[5..]).expect_err("refused");
    assert_eq!(short, CalibrationError::NotANumber("the distance M8".into()));
    assert_eq!(
        holey::parse_measurements(&form(&[1.0; 11])),
        Err(CalibrationError::NotANumber("the distance M12".into()))
    );
}

#[test]
fn the_fit_refuses_when_the_kinematics_cannot_place_a_hole() {
    let broken = settings(&[("Maslow Settings", "sledWeight", "nan")]);
    let refused = holey::calibrate_measurements(&broken, &holey::ideal_lengths(&settings(&[]))).expect_err("refused");
    assert_eq!(refused, CalibrationError::NoSolution);
    assert_eq!(refused.to_string(), "The calibration could not be calculated from these measurements.");
    assert_eq!(holey::simulate_measurements(&broken, 2990.0, 470.0, 0.0, 0.0), None);
}

#[test]
fn the_fit_refuses_measurements_that_are_not_finite() {
    let mut measured = holey::ideal_lengths(&settings(&[]));
    measured[3] = f64::NAN;
    assert_eq!(holey::calibrate_measurements(&settings(&[]), &measured), Err(CalibrationError::NoSolution));
}

#[test]
fn holey_accept_saves_the_four_values_as_python_prints_them() {
    let result = holey::HoleyResult {
        motor_y_offset: 470.0,
        distance_between_motors: 2990.0,
        left_chain_tolerance: 0.3,
        right_chain_tolerance: -0.2,
        total_error: 0.0,
    };
    let accept = result.accept();
    let write = |section, key, value: &str| SettingWrite { section, key, value: value.into() };
    assert_eq!(
        accept.writes,
        [
            write("Maslow Settings", "motorOffsetY", "470.0"),
            write("Maslow Settings", "motorSpacingX", "2990.0"),
            write("Advanced Settings", "leftChainTolerance", "0.3"),
            write("Advanced Settings", "rightChainTolerance", "-0.2"),
        ]
    );
    assert!(accept.gcode.is_empty());
    let unchanged =
        store(&[("Maslow Settings", "motorOffsetY", "470"), ("Maslow Settings", "motorSpacingX", "2990.0")]);
    assert_eq!(accept.queued(lookup(&unchanged)).unwrap(), ["$40=0.3", "$41=-0.2"]);
}

fn holey_pattern(units: &str) -> Vec<String> {
    let holes = [
        ("0", "355.6"),
        ("-965.2", "355.6"),
        ("-965.2", "-355.6"),
        ("0", "-355.6"),
        ("965.2", "-355.6"),
        ("965.2", "355.6"),
    ];
    let cuts = holes.iter().flat_map(|(x, y)| [format!("G0 X{x} Y{y}"), "G1 Z-5 F500".into(), "G0 Z5".into()]);
    let head = ["G21", "G90", "G40", "G17", "G0 Z5", "M3"].map(String::from);
    let tail = ["G0 X0 Y0", "M5"].map(String::from);
    let inches = (units == "INCHES").then(|| "G20".to_string());
    head.into_iter().chain(cuts).chain(tail).chain(inches).collect()
}

#[test]
fn the_holey_pattern_raises_the_bit_before_moving_and_plunges_at_a_feed_rate() {
    assert_eq!(holey::test_pattern(&settings(&[]), false), holey_pattern("MM"));
    assert_eq!(holey::test_pattern(&settings(&[]), true), holey_pattern("INCHES"));
}

fn triangular_form() -> Value {
    json!({"cut12": 1930, "cut34": 1930, "cut5": 300, "bitDiameter": 6.35})
}

#[test]
fn triangular_refuses_a_field_that_is_not_a_number() {
    let cases = [
        ("cut12", "the distance between cut 1 and cut 2"),
        ("cut34", "the distance between cut 3 and cut 4"),
        ("cut5", "the distance between the top of the work area and cut 5"),
        ("bitDiameter", "the bit diameter"),
    ];
    cases.iter().for_each(|(key, label)| {
        let mut form = triangular_form();
        form[*key] = json!("nope");
        let refused = triangular::calculate(&settings(&[]), &form).expect_err("refused");
        assert_eq!(refused.to_string(), format!("Please enter a number for {label}."));
    });
}

#[test]
fn triangular_refuses_a_measurement_out_of_range() {
    let cases = [
        ("cut12", json!(3000), "The measurement between cut 1 and cut 2 of 3000.0 mm seems wrong."),
        ("cut12", json!(1000), "The measurement between cut 1 and cut 2 of 1000.0 mm seems wrong."),
        ("cut34", json!(2500.5), "The measurement between cut 3 and cut 4 of 2500.5 mm seems wrong."),
        ("cut5", json!(-1), "The measurement between the top edge of the work area and cut 5 of -1.0 mm seems wrong."),
        (
            "cut5",
            json!(700),
            "The measurement between the top edge of the work area and cut 5 of 700.0 mm seems wrong.",
        ),
        ("bitDiameter", json!(30), "The bit diameter value of 30.0 mm seems wrong."),
        ("bitDiameter", json!(-0.5), "The bit diameter value of -0.5 mm seems wrong."),
    ];
    cases.iter().for_each(|(key, value, message)| {
        let mut form = triangular_form();
        form[*key] = value.clone();
        let refused = triangular::calculate(&settings(&[]), &form).expect_err("refused");
        assert_eq!(refused.to_string(), format!("{message}\n\nPlease check the number and enter it again."));
    });
}

#[test]
fn triangular_accept_saves_three_values_then_rises_and_returns_to_the_centre() {
    let result = triangular::calculate(&settings(&[]), &triangular_form()).expect("calculated");
    let accept = result.accept(5.0);
    let values: Vec<&str> = accept.writes.iter().map(|write| write.value.as_str()).collect();
    assert_eq!(values, ["400.6", "138.8", "33.443549"]);
    assert_eq!(accept.gcode, ["G21 ", "G90 ", "G40 ", "G0 Z5.0 ", "G0 X0 Y0 "]);
    assert_eq!(
        accept.queued(lookup(&store(&[]))).unwrap(),
        ["$3=400.6", "$8=138.8", "$37=33.443549", "G21 ", "G90 ", "G40 ", "G0 Z5.0 ", "G0 X0 Y0 "]
    );
    assert_eq!(result.accept(f64::NAN).queued(lookup(&store(&[]))), Err(CalibrationError::NotFinite));
}

#[test]
fn a_top_fed_machine_calibrates_like_python() {
    let top = settings(&[("Advanced Settings", "chainOverSprocket", "Top")]);
    let result = triangular::calculate(&top, &triangular_form()).expect("calculated");
    assert_eq!((result.motor_y_offset, result.rotation_radius, result.chain_sag_correction), (400.6, 138.8, 33.420469));
    assert!((result.cut34_y_offset + 0.1265886376542653).abs() < 1e-6);
}

#[test]
fn triangular_refuses_a_machine_whose_chain_lengths_are_not_finite() {
    let wide = settings(&[("Maslow Settings", "motorSpacingX", "1e200")]);
    assert_eq!(triangular::calculate(&wide, &triangular_form()), Err(CalibrationError::NoSolution));
}

#[test]
fn triangular_gives_up_after_5000_iterations_like_python() {
    let sagging = settings(&[("Advanced Settings", "chainSagCorrection", "1e300")]);
    assert_eq!(triangular::calculate(&sagging, &triangular_form()), Err(CalibrationError::NotCalibrated));
}

#[test]
fn the_triangular_pattern_prints_its_moves_like_python() {
    let expected = |numbers: [&str; 5], inches: bool| -> Vec<String> {
        let [x, y, down, right, up] = numbers;
        let lines = [
            "G21 ",
            "G90 ",
            "G40 ",
            "G0 Z5 ",
            "G0 X0 Y0 ",
            "G17 ",
            "G91 ",
            "M3 ",
            &format!("G0 X-{x} Y+{y} "),
            "G1 Z-7 F500 ",
            "G1 Y-25.4 ",
            "G1 Z7 ",
            &format!("G0 Y-{down} "),
            "G1 Z-7 ",
            "G1 Y-25.4 ",
            "G1 Z7 ",
            &format!("G0 X{right} Y{up} "),
            "G1 Z-7 ",
            "G1 X25.4 ",
            "G1 Y-25.4 ",
            "G1 Z7 ",
            &format!("G0 Y-{down} "),
            "G1 Z-7 ",
            "G1 Y-25.4 ",
            "G1 Z7 ",
            "M5 ",
            "G90 ",
            "G0 X0 Y0 ",
        ]
        .map(String::from);
        lines.into_iter().chain(inches.then(|| "G20 ".to_string())).collect()
    };
    let default = ["965.2", "368.3", "685.8000000000001", "1905.0", "736.6"];
    assert_eq!(triangular::test_pattern(&settings(&[]), false), expected(default, false));
    let small = settings(&[("Maslow Settings", "bedWidth", "2000"), ("Maslow Settings", "bedHeight", "1000")]);
    assert_eq!(triangular::test_pattern(&small, true), expected(["746.0", "258.7", "466.6", "1466.6", "517.4"], true));
}

#[test]
fn a_taut_chain_calibrates_like_python_until_the_sag_would_go_negative() {
    let taut = settings(&[("Advanced Settings", "chainSagCorrection", "0")]);
    let form = |cut34| json!({"cut12": 1930, "cut34": cut34, "cut5": 300, "bitDiameter": 6.35});
    let result = triangular::calculate(&taut, &form(1920)).expect("calculated");
    assert_eq!((result.motor_y_offset, result.rotation_radius, result.chain_sag_correction), (404.1, 141.6, 50.204149));
    assert!((result.cut34_y_offset - 2.2797590860887453).abs() < 1e-6);
    assert_eq!(triangular::calculate(&taut, &form(1935)), Err(CalibrationError::NotCalibrated));
}

#[test]
fn every_refusal_has_python_s_message() {
    assert_eq!(
        CalibrationError::NotCalibrated.to_string(),
        "The machine was not able to be calibrated. Please ensure the work area dimensions are correct and try again."
    );
}

#[test]
fn a_setting_that_is_not_finite_never_becomes_a_result_to_accept() {
    let triangular_cases = [("Maslow Settings", "motorOffsetY", "nan"), ("Maslow Settings", "motorOffsetY", "inf")];
    triangular_cases.iter().for_each(|setting| {
        let refused = triangular::calculate(&settings(&[*setting]), &triangular_form());
        assert_eq!(refused, Err(CalibrationError::NoSolution), "{setting:?}");
    });
    let configured = settings(&[]);
    let broken = settings(&[("Advanced Settings", "leftChainTolerance", "inf")]);
    let refused = holey::calibrate_measurements(&broken, &holey::ideal_lengths(&configured));
    assert_eq!(refused, Err(CalibrationError::NoSolution));
}
