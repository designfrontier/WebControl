//! `tests/fixtures/calibration/kinematics.json`, with the README's rules: floats within `tolerance`, quadrilateral
//! cases on one fresh model in file order, `null` where no position matches.

use serde_json::Value;
use wc_kinematics::{ChainFeed, Geometry, Kinematics, Mode};

fn fixture() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../tests/fixtures/calibration/kinematics.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json")
}

fn number(value: &Value) -> f64 {
    value.as_f64().expect("a number")
}

fn geometry(parameters: &Value) -> Geometry {
    let get = |key: &str| number(&parameters[key]);
    Geometry {
        d: get("D"),
        r: get("R"),
        machine_width: get("machineWidth"),
        machine_height: get("machineHeight"),
        motor_offset_y: get("motorOffsetY"),
        rotation_disk_radius: get("rotationDiskRadius"),
        sled_weight: get("sledWeight"),
        chain_density: get("chainDensity"),
        chain_elasticity: get("chainElasticity"),
        left_chain_tolerance: get("leftChainTolerance"),
        right_chain_tolerance: get("rightChainTolerance"),
        chain_over_sprocket: ChainFeed::from_setting(get("chainOverSprocket") as i64),
        l: get("l"),
        s: get("s"),
        h3: get("h3"),
        mode: if parameters["isQuadKinematics"].as_bool().expect("bool") {
            Mode::Quadrilateral
        } else {
            Mode::Triangular
        },
        ..Geometry::default()
    }
}

fn close(actual: f64, expected: &Value, tolerance: f64, what: &str) {
    let expected = number(expected);
    assert!((actual - expected).abs() <= tolerance, "{what}: {actual} != {expected}");
}

fn check_position(actual: Option<(f64, f64)>, expected: &Value, tolerance: f64, what: &str) {
    match (actual, expected) {
        (None, Value::Null) => {}
        (Some((x, y)), Value::Array(pair)) => {
            close(x, &pair[0], tolerance, what);
            close(y, &pair[1], tolerance, what);
        }
        _ => panic!("{what}: {actual:?} != {expected}"),
    }
}

fn check_mode(mode: &Value, tolerance: f64, guess: (f64, f64)) {
    let name = mode["mode"].as_str().expect("name");
    let mut kinematics = Kinematics::new(geometry(&mode["parameters"]));
    mode["points"].as_array().expect("points").iter().for_each(|point| {
        let what = format!("{name} {point}");
        let (left, right) = kinematics.inverse(number(&point["x"]), number(&point["y"])).expect("chains");
        close(left, &point["left"], tolerance, &what);
        close(right, &point["right"], tolerance, &what);
        check_position(kinematics.forward_from(left, right, guess), &point["forward"], tolerance, &what);
    });
    mode["offBed"].as_array().expect("offBed").iter().for_each(|point| {
        let what = format!("{name} off bed {point}");
        let (left, right) = kinematics.inverse(number(&point["x"]), number(&point["y"])).expect("chains");
        close(left, &point["chains"][0], tolerance, &what);
        close(right, &point["chains"][1], tolerance, &what);
    });
    mode["unreachable"].as_array().expect("unreachable").iter().for_each(|point| {
        let what = format!("{name} unreachable {point}");
        let found = kinematics.forward_from(number(&point["left"]), number(&point["right"]), guess);
        check_position(found, &point["forward"], tolerance, &what);
    });
}

#[test]
fn both_modes_reproduce_the_python_fixture() {
    let fixture = fixture();
    let tolerance = number(&fixture["tolerance"]);
    let guess = (number(&fixture["forwardGuess"][0]), number(&fixture["forwardGuess"][1]));
    let modes = fixture["modes"].as_array().expect("modes");
    assert_eq!(modes.len(), 2);
    modes.iter().for_each(|mode| check_mode(mode, tolerance, guess));
}

#[test]
fn the_default_forward_guess_is_the_fixture_guess() {
    let fixture = fixture();
    let mode = &fixture["modes"][0];
    let point = &mode["points"][40];
    let mut kinematics = Kinematics::new(geometry(&mode["parameters"]));
    let (left, right) = kinematics.inverse(number(&point["x"]), number(&point["y"])).expect("chains");
    check_position(kinematics.forward(left, right), &point["forward"], number(&fixture["tolerance"]), "centre");
}
