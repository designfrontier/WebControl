//! Triangular and holey calibration through the machine: Calculate, the result guard, Accept's `$` lines and the
//! test patterns (ported from test_api_calibration, test_calibration_accept, test_calibration_routes and
//! test_holey_test_pattern).

mod rig;

use rig::{Rig, alerts, settings};
use serde_json::{Value, json};
use wc_calibration::MachineSettings;
use wc_config::Settings;
use wc_machine::calibration::{Kind, calculate, holey_parameters, triangular_parameters};
use wc_machine::{Effect, Event, Request, Response};
use wc_testkit::contract::{Contract, ResponseBody};

const TRIANGULAR: &str = r#"{"cut12": 1930, "cut34": "1930", "cut5": 300, "bitDiameter": 6.35}"#;

fn triangular_body() -> Value {
    serde_json::from_str(TRIANGULAR).unwrap()
}

fn path(kind: Kind) -> &'static str {
    match kind {
        Kind::Triangular => "/api/calibration/triangular",
        Kind::Holey => "/api/calibration/holey",
    }
}

/// Answers a request and checks its status and body, and a body it accepted, against the contract.
fn respond(rig: &mut Rig, path: &str, body: Option<&Value>, request: Request) -> Response {
    let effects = rig.send(Event::Request { id: 3, request });
    let response = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::Respond { id: 3, response } => Some(response.clone()),
            _ => None,
        })
        .expect("every request is answered");
    let contract = Contract::shared();
    contract.validate_response("POST", path, response.status, &ResponseBody::Json(response.body.clone())).unwrap();
    if let Some(body) = body.filter(|_| response.status < 400) {
        contract.validate_request("POST", path, body).unwrap();
    }
    response
}

fn calculate_on(rig: &mut Rig, kind: Kind, body: Value) -> Response {
    let calculated = calculate(kind, &rig.machine.snapshot().settings, &body);
    respond(rig, path(kind), Some(&body), Request::Calibrated(calculated))
}

fn accept_on(rig: &mut Rig, kind: Kind) -> Response {
    let path = format!("{}/accept", path(kind));
    respond(rig, &path, Some(&json!({})), Request::AcceptCalibration(kind))
}

fn machine_settings(settings: &Settings) -> MachineSettings {
    MachineSettings::from_lookup(|section, key| settings.text(section, key)).unwrap()
}

fn ideal(settings: &Settings) -> Value {
    json!({ "measurements": wc_calibration::holey::ideal_lengths(&machine_settings(settings)).to_vec() })
}

fn text(rig: &Rig, section: &str, key: &str) -> String {
    rig.machine.snapshot().settings.text(section, key).unwrap()
}

#[test]
fn the_parameters_read_the_stored_settings() {
    let defaults = settings(&[("Advanced Settings", "rightChainTolerance", json!("0.25"))]);
    let triangular = serde_json::to_value(triangular_parameters(&defaults)).unwrap();
    assert_eq!(triangular, json!({"motorYoffset": 463.0, "rotationRadius": 139.1, "chainSagCorrection": 33.0}));
    let holey = serde_json::to_value(holey_parameters(&defaults)).unwrap();
    assert_eq!(
        holey,
        json!({"motorYoffset": 463.0, "distanceBetweenMotors": 2978.4, "leftChainTolerance": 0.0,
            "rightChainTolerance": 0.25})
    );
}

#[test]
fn triangular_calculates_without_saving_or_sending() {
    let mut rig = Rig::connected();
    let response = calculate_on(&mut rig, Kind::Triangular, triangular_body());
    assert_eq!(response.status, 200);
    let motor = response.body["motorYoffset"].as_f64().unwrap();
    assert!((motor - 400.6).abs() < 5.0, "{motor}");
    assert!(response.body["calibrationError"].is_number());
    assert_eq!(text(&rig, "Maslow Settings", "motorOffsetY"), "463");
    assert_eq!(rig.run_to_quiet(), Vec::<String>::new());
}

#[test]
fn triangular_refuses_bad_measurements_with_python_s_messages() {
    let cases = [
        (json!({"cut12": "abc"}), "Please enter a number for the distance between cut 1 and cut 2."),
        (json!({"cut34": null}), "Please enter a number for the distance between cut 3 and cut 4."),
        (json!({"cut12": 3000}), "The measurement between cut 1 and cut 2 of 3000.0 mm seems wrong."),
        (json!({"cut34": 100}), "The measurement between cut 3 and cut 4 of 100.0 mm seems wrong."),
        (
            json!({"cut5": -1}),
            "The measurement between the top edge of the work area and cut 5 of -1.0 mm seems wrong.",
        ),
        (json!({"bitDiameter": 30}), "The bit diameter of 30.0 mm seems wrong."),
    ];
    let mut rig = Rig::new();
    cases.into_iter().for_each(|(change, message)| {
        let mut body = triangular_body();
        change.as_object().unwrap().iter().for_each(|(key, value)| body[key] = value.clone());
        let response = calculate_on(&mut rig, Kind::Triangular, body);
        assert_eq!(response.status, 400);
        let error = response.body["error"].as_str().unwrap();
        assert!(error.starts_with(message), "{error}");
        assert!(error.ends_with(" Please check the number and enter it again.") || error.starts_with("Please"));
    });
    assert_eq!(calculate_on(&mut rig, Kind::Triangular, json!([1, 2])).status, 400);
}

#[test]
fn triangular_answers_422_when_it_cannot_calibrate() {
    let error = "The machine could not be calibrated. Check the work area dimensions and try again.";
    [("Maslow Settings", "motorSpacingX", "abc"), ("Advanced Settings", "chainSagCorrection", "nan")]
        .into_iter()
        .for_each(|(section, key, value)| {
            let mut rig = Rig::with(settings(&[(section, key, json!(value))]));
            let response = calculate_on(&mut rig, Kind::Triangular, triangular_body());
            assert_eq!((response.status, response.body), (422, json!({ "error": error })), "{key}");
        });
}

#[test]
fn holey_calculates_from_twelve_measurements() {
    let mut rig = Rig::new();
    let body = ideal(&rig.machine.snapshot().settings);
    let response = calculate_on(&mut rig, Kind::Holey, body);
    assert_eq!(response.status, 200);
    assert_eq!(response.body["distanceBetweenMotors"], 2978.4);
    assert_eq!(response.body["motorYoffset"], 463.0);
    assert!(response.body["calibrationError"].as_f64().unwrap() < 1e-6);
}

#[test]
fn holey_refuses_bad_measurements() {
    let mut rig = Rig::new();
    let mut twelfth_bad = vec![json!(1); 11];
    twelfth_bad.push(json!("x"));
    [
        (json!({}), "Please enter all 12 measurements."),
        (json!({"measurements": vec![1; 11]}), "Please enter all 12 measurements."),
        (json!({ "measurements": twelfth_bad }), "Please enter a number for the distance M12."),
    ]
    .into_iter()
    .for_each(|(body, message)| {
        let response = calculate_on(&mut rig, Kind::Holey, body);
        assert_eq!((response.status, response.body), (400, json!({ "error": message })));
    });
}

#[test]
fn holey_answers_422_when_no_fit_is_found() {
    let error = json!({"error": "The calibration could not be calculated from these measurements."});
    [("Maslow Settings", "sledWeight", "nan"), ("Maslow Settings", "motorSpacingX", "abc")].into_iter().for_each(
        |(section, key, value)| {
            let mut rig = Rig::with(settings(&[(section, key, json!(value))]));
            let response = calculate_on(&mut rig, Kind::Holey, ideal(&Settings::defaults()));
            assert_eq!((response.status, &response.body), (422, &error), "{key}");
        },
    );
}

#[test]
fn accept_refuses_until_something_is_calculated() {
    let mut rig = Rig::connected();
    [Kind::Triangular, Kind::Holey].into_iter().for_each(|kind| {
        let response = accept_on(&mut rig, kind);
        assert_eq!(response.status, 409);
        assert!(response.body["error"].as_str().unwrap().contains("Calculate"));
    });
    assert_eq!(rig.run_to_quiet(), Vec::<String>::new());
    assert_eq!(text(&rig, "Maslow Settings", "motorOffsetY"), "463");
}

/// The holey_calibration_accept_writes_settings transcript's measurements: Python's values to the last digit.
#[test]
fn holey_accept_saves_the_fit_and_writes_each_changed_setting_once() {
    let mut rig = Rig::connected();
    let measurements = [966.0, 964.5, 965.8, 964.9, 711.6, 710.9, 711.4, 1199.5, 1198.2, 1199.1, 1198.6, 254.3];
    let calculated = calculate_on(&mut rig, Kind::Holey, json!({ "measurements": measurements }));
    assert_eq!(calculated.body["calibrationError"], 1.292957621842212);
    let accepted = accept_on(&mut rig, Kind::Holey);
    assert_eq!(
        accepted.body,
        json!({"motorYoffset": 462.64616, "distanceBetweenMotors": 2978.54418, "leftChainTolerance": -0.03767,
            "rightChainTolerance": 0.04275})
    );
    assert_eq!(rig.run_to_quiet(), ["$3=462.64616", "$2=2978.54418", "$40=-0.03767", "$41=0.04275"]);
    assert_eq!(text(&rig, "Maslow Settings", "motorSpacingX"), "2978.54418");
    assert_eq!(accept_on(&mut rig, Kind::Holey).status, 409);
}

#[test]
fn triangular_accept_saves_then_returns_to_the_centre_in_mm() {
    let mut rig = Rig::connected();
    let calculated = calculate_on(&mut rig, Kind::Triangular, triangular_body()).body;
    let accepted = accept_on(&mut rig, Kind::Triangular);
    assert_eq!(accepted.status, 200);
    ["motorYoffset", "rotationRadius", "chainSagCorrection"]
        .iter()
        .for_each(|key| assert_eq!(accepted.body[key], calculated[key], "{key}"));
    let sent = rig.run_to_quiet();
    assert!(sent[0].starts_with("$3="), "{sent:?}");
    assert_eq!(sent[sent.len() - 4..], ["G21", "G90", "G40", "G0 X0 Y0"]);
    assert_eq!(accept_on(&mut rig, Kind::Triangular).status, 409);
}

#[test]
fn a_refused_recalculation_withdraws_the_earlier_result() {
    let mut rig = Rig::new();
    assert_eq!(calculate_on(&mut rig, Kind::Triangular, triangular_body()).status, 200);
    let mut wrong = triangular_body();
    wrong["bitDiameter"] = json!(30);
    assert_eq!(calculate_on(&mut rig, Kind::Triangular, wrong).status, 400);
    assert_eq!(accept_on(&mut rig, Kind::Triangular).status, 409);
    let body = ideal(&rig.machine.snapshot().settings);
    assert_eq!(calculate_on(&mut rig, Kind::Holey, body.clone()).status, 200);
    let mut short = body;
    short["measurements"].as_array_mut().unwrap().pop();
    assert_eq!(calculate_on(&mut rig, Kind::Holey, short).status, 400);
    assert_eq!(accept_on(&mut rig, Kind::Holey).status, 409);
}

#[test]
fn accept_waits_for_the_job_to_stop() {
    let mut rig = Rig::connected();
    calculate_on(&mut rig, Kind::Triangular, triangular_body());
    rig.machine.load_gcode("G0 X1\nG0 X2\nG0 X3\n");
    rig.act("startRun");
    let refused = accept_on(&mut rig, Kind::Triangular);
    assert_eq!(refused.body, json!({"error": "Stop the job before accepting a calibration result."}));
    rig.act("stopRun");
    rig.advance(400);
    rig.rx("<Idle,MPos:0.00,0.00,0.00,WPos:0.000,0.000,0.000>");
    assert_eq!(accept_on(&mut rig, Kind::Triangular).status, 200);
}

/// Triangular Accept ends with `G0 X0 Y0`, which must not queue behind a probe that leaves the bit on the plate.
#[test]
fn accept_waits_for_touch_z_to_finish() {
    let mut rig = Rig::connected();
    calculate_on(&mut rig, Kind::Triangular, triangular_body());
    rig.act("touchZ");
    let refused = accept_on(&mut rig, Kind::Triangular);
    assert_eq!((refused.status, refused.body), (409, json!({"error": "Wait for Touch Z to finish or press Stop"})));
    assert_eq!(text(&rig, "Maslow Settings", "motorOffsetY"), "463");
    assert_eq!(rig.run_to_quiet(), ["G90 G38.2 Z-10"], "the probe answers and nothing follows it");
    assert_eq!(accept_on(&mut rig, Kind::Triangular).status, 200);
    assert_eq!(rig.run_to_quiet().last().map(String::as_str), Some("G0 X0 Y0"));
}

const HOLES: [(&str, &str); 6] = [
    ("0", "355.6"),
    ("-965.2", "355.6"),
    ("-965.2", "-355.6"),
    ("0", "-355.6"),
    ("965.2", "-355.6"),
    ("965.2", "355.6"),
];

fn holey_pattern(inches: bool) -> Vec<String> {
    let cuts = HOLES.iter().flat_map(|(x, y)| [format!("G0 X{x} Y{y}"), "G1 Z-5 F500".into(), "G0 Z5".into()]);
    let head = ["G21", "G90", "G40", "G17", "G0 Z5", "M3"].map(String::from);
    let tail = ["G0 X0 Y0".to_string(), "M5".to_string()].into_iter().chain(inches.then(|| "G20".to_string()));
    head.into_iter().chain(cuts).chain(tail).collect()
}

/// The bit goes up before every move and plunges at a feed rate, ported from test_holey_test_pattern.
#[test]
fn the_holey_pattern_raises_the_bit_before_moving_and_plunges_at_a_feed_rate() {
    [(false, "MM"), (true, "INCHES")].into_iter().for_each(|(inches, units)| {
        let mut rig = Rig::connected_with(settings(&[("Computed Settings", "units", json!(units))]));
        assert_eq!(alerts(&rig.act("cutHoleyCalibrationPattern")), Vec::<String>::new());
        assert_eq!(rig.run_to_quiet(), holey_pattern(inches), "{units}");
    });
}

#[test]
fn the_triangular_pattern_cuts_five_marks_in_relative_moves() {
    let mut rig = Rig::connected();
    rig.act("cutTriangularCalibrationPattern");
    let sent = rig.run_to_quiet();
    assert_eq!(sent[..9], ["G21", "G90", "G40", "G0 Z5", "G0 X0 Y0", "G17", "G91", "M3", "G0 X-965.2 Y+368.3"]);
    assert_eq!(sent[sent.len() - 3..], ["M5", "G90", "G0 X0 Y0"]);
    assert_eq!(sent.len(), 28);
    assert_eq!(rig.machine.snapshot().state_fields()["positioningMode"], "G90");
}

#[test]
fn the_patterns_need_the_controller_and_valid_settings() {
    let mut rig = Rig::new();
    ["cutHoleyCalibrationPattern", "cutTriangularCalibrationPattern"].iter().for_each(|command| {
        assert_eq!(alerts(&rig.act(command)), ["The controller is not connected."], "{command}");
    });
    let mut broken = Rig::connected_with(settings(&[("Maslow Settings", "bedWidth", json!("wide"))]));
    assert_eq!(alerts(&broken.act("cutHoleyCalibrationPattern")), ["Error with cutting holey calibration pattern."]);
    assert_eq!(
        alerts(&broken.act("cutTriangularCalibrationPattern")),
        ["Error with cutting triangular calibration pattern."]
    );
    assert_eq!(broken.run_to_quiet(), Vec::<String>::new());
}

#[test]
fn the_accept_actions_save_the_result_or_alert() {
    let mut rig = Rig::connected();
    assert_eq!(alerts(&rig.act("acceptHoleyCalibrationResults")), ["Error with accepting holey calibration results."]);
    assert_eq!(
        alerts(&rig.act("acceptTriangularCalibrationResults")),
        ["Error with accepting triangular calibration results."]
    );
    let body = ideal(&rig.machine.snapshot().settings);
    calculate_on(&mut rig, Kind::Holey, body);
    assert_eq!(alerts(&rig.act("acceptHoleyCalibrationResults")), Vec::<String>::new());
    calculate_on(&mut rig, Kind::Triangular, triangular_body());
    assert_eq!(alerts(&rig.act("acceptTriangularCalibrationResults")), Vec::<String>::new());
    assert_eq!(rig.run_to_quiet()[..2], ["$3=400.6", "$8=138.8"]);
}
