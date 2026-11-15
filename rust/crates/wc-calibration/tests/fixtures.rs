//! `tests/fixtures/calibration/{holey_calibrate,triangular_calculate,accept_commands}.json`, with the README's
//! rules: floats within `tolerance`, `keyTolerance` first, `null` expected means refused, accept line for line.

use serde_json::{Value, json};
use wc_calibration::{Accept, MachineSettings, holey, triangular};

fn fixture(name: &str) -> Value {
    let path = format!("{}/../../../tests/fixtures/calibration/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json")
}

fn lookup(geometry: &Value) -> impl Fn(&str, &str) -> Option<String> + '_ {
    move |section, key| geometry[section][key].as_str().map(str::to_string)
}

fn settings(geometry: &Value) -> MachineSettings {
    MachineSettings::from_lookup(lookup(geometry)).expect("settings")
}

fn tolerance_for(fixture: &Value, key: &str) -> f64 {
    fixture["keyTolerance"][key].as_f64().or_else(|| fixture["tolerance"].as_f64()).expect("tolerance")
}

fn check(fixture: &Value, actual: &Value, expected: &Value, what: &str) {
    let fields = expected.as_object().expect("expected fields");
    assert_eq!(actual.as_object().expect("actual fields").len(), fields.len(), "{what}");
    fields.iter().for_each(|(key, expected)| {
        let (actual, expected) = (actual[key].as_f64().expect("number"), expected.as_f64().expect("number"));
        let tolerance = tolerance_for(fixture, key);
        assert!((actual - expected).abs() <= tolerance, "{what} {key}: {actual} != {expected} (±{tolerance})");
    });
}

fn holey_json(result: &holey::HoleyResult) -> Value {
    json!({
        "Opt_D": result.distance_between_motors,
        "Opt_motorOffsetY": result.motor_y_offset,
        "Opt_leftChainTolerance": result.left_chain_tolerance,
        "Opt_rightChainTolerance": result.right_chain_tolerance,
        "totalError": result.total_error,
    })
}

#[test]
fn holey_calibrate_reproduces_the_python_fixture() {
    let fixture = fixture("holey_calibrate.json");
    let settings = settings(&fixture["geometry"]);
    let cases = fixture["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 8);
    cases.iter().for_each(|case| {
        let name = case["name"].as_str().expect("name");
        let measurements = case["measurements"].as_array().expect("measurements");
        match (holey::calibrate(&settings, measurements), &case["expected"]) {
            (Err(_), Value::Null) => {}
            (Ok(result), expected @ Value::Object(_)) => check(&fixture, &holey_json(&result), expected, name),
            (result, expected) => panic!("{name}: {result:?} != {expected}"),
        }
    });
}

/// Stricter than the fixture's 1e-3: scipy's own algorithm stops where Python did, so Accept sends the same bytes.
#[test]
fn holey_calibrate_lands_on_python_s_five_decimals_exactly() {
    let fixture = fixture("holey_calibrate.json");
    let settings = settings(&fixture["geometry"]);
    fixture["cases"].as_array().expect("cases").iter().filter(|case| case["expected"].is_object()).for_each(|case| {
        let result = holey::calibrate(&settings, case["measurements"].as_array().expect("measurements")).unwrap();
        let (actual, expected) = (holey_json(&result), &case["expected"]);
        ["Opt_D", "Opt_motorOffsetY", "Opt_leftChainTolerance", "Opt_rightChainTolerance"]
            .iter()
            .for_each(|key| assert_eq!(actual[key], expected[key], "{} {key}", case["name"]));
        let error = (actual["totalError"].as_f64().unwrap() - expected["totalError"].as_f64().unwrap()).abs();
        assert!(error < 1e-9, "{} totalError off by {error}", case["name"]);
    });
}

#[test]
fn the_holey_model_is_the_fixture_model() {
    let fixture = fixture("holey_calibrate.json");
    let geometry = holey::model(&settings(&fixture["geometry"]));
    let k = &fixture["kinematics"];
    let pairs = [
        (geometry.d, &k["D"]),
        (geometry.r, &k["R"]),
        (geometry.machine_width, &k["machineWidth"]),
        (geometry.machine_height, &k["machineHeight"]),
        (geometry.motor_offset_y, &k["motorOffsetY"]),
        (geometry.rotation_disk_radius, &k["rotationDiskRadius"]),
        (geometry.sled_weight, &k["sledWeight"]),
        (geometry.chain_density, &k["chainDensity"]),
        (geometry.chain_elasticity, &k["chainElasticity"]),
        (geometry.left_chain_tolerance, &k["leftChainTolerance"]),
        (geometry.right_chain_tolerance, &k["rightChainTolerance"]),
        (geometry.l, &k["l"]),
        (geometry.s, &k["s"]),
        (geometry.h3, &k["h3"]),
    ];
    pairs.iter().for_each(|(actual, expected)| assert_eq!(Some(*actual), expected.as_f64(), "{expected}"));
    assert_eq!(
        geometry.chain_over_sprocket,
        wc_kinematics::ChainFeed::from_setting(k["chainOverSprocket"].as_i64().expect("int"))
    );
    assert_eq!(geometry.mode, wc_kinematics::Mode::Triangular);
}

#[test]
fn triangular_calculate_reproduces_the_python_fixture() {
    let fixture = fixture("triangular_calculate.json");
    let settings = settings(&fixture["geometry"]);
    fixture["cases"].as_array().expect("cases").iter().for_each(|case| {
        let inputs = &case["inputs"];
        let what = inputs.to_string();
        match (triangular::calculate(&settings, inputs), &case["expected"]) {
            (Err(_), Value::Null) => {}
            (Ok(result), expected @ Value::Object(_)) => {
                let actual = json!({
                    "motorYoffsetEst": result.motor_y_offset,
                    "rotationRadiusEst": result.rotation_radius,
                    "chainSagCorrectionEst": result.chain_sag_correction,
                    "cut34YoffsetEst": result.cut34_y_offset,
                });
                check(&fixture, &actual, expected, &what);
            }
            (result, expected) => panic!("{what}: {result:?} != {expected}"),
        }
    });
}

fn described(accept: &Accept, geometry: &Value) -> Vec<Value> {
    let units = |key: u32| match key {
        40 | 41 => "%",
        37 => "",
        _ => "mm",
    };
    accept
        .queued(lookup(geometry))
        .iter()
        .map(|line| match line.strip_prefix('$').and_then(|rest| rest.split_once('=')) {
            Some((key, value)) => {
                let key: u32 = key.parse().expect("key");
                json!({"command": line, "key": key, "value": value, "units": units(key)})
            }
            None => json!({"gcode": line}),
        })
        .collect()
}

#[test]
fn accept_queues_the_python_fixture_commands() {
    let fixture = fixture("accept_commands.json");
    let geometry = &fixture["geometry"];
    let settings = settings(geometry);
    let truth = &fixture["holey"]["trueMachine"];
    let measurements = holey::simulate_measurements(
        &settings,
        truth["D"].as_f64().expect("D"),
        truth["motorOffsetY"].as_f64().expect("offset"),
        truth["leftChainTolerance"].as_f64().expect("left"),
        truth["rightChainTolerance"].as_f64().expect("right"),
    )
    .expect("measurements");
    let measurements: Vec<Value> = measurements.iter().map(|m| json!(m)).collect();
    let holey = holey::calibrate(&settings, &measurements).expect("holey");
    assert_eq!(described(&holey.accept(), geometry), *fixture["holey"]["queued"].as_array().expect("queued"));

    let triangular = triangular::calculate(&settings, &fixture["triangular"]["inputs"]).expect("triangular");
    assert_eq!(described(&triangular.accept(), geometry), *fixture["triangular"]["queued"].as_array().expect("queued"));
}

#[test]
fn the_exact_measurements_are_the_fixture_measurements() {
    let fixture = fixture("holey_calibrate.json");
    let truth = &fixture["trueMachine"];
    let simulated = holey::simulate_measurements(
        &settings(&fixture["geometry"]),
        truth["D"].as_f64().expect("D"),
        truth["motorOffsetY"].as_f64().expect("offset"),
        truth["leftChainTolerance"].as_f64().expect("left"),
        truth["rightChainTolerance"].as_f64().expect("right"),
    )
    .expect("measurements");
    let expected = fixture["cases"][1]["measurements"].as_array().expect("measurements");
    simulated.iter().zip(expected).for_each(|(actual, expected)| {
        assert!((actual - expected.as_f64().expect("number")).abs() < 1e-9, "{actual} != {expected}");
    });
    let ideal = holey::ideal_lengths(&settings(&fixture["geometry"]));
    let expected = fixture["cases"][0]["measurements"].as_array().expect("measurements");
    ideal.iter().zip(expected).for_each(|(actual, expected)| assert_eq!(Some(*actual), expected.as_f64()));
}
