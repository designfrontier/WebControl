//! Settings writes through the machine: the settings screens' save (ported from test_api_settings), quick configure
//! (test_api_machine), the PID test request (test_api_system), the unit switches and saving to disk.

mod rig;

use std::sync::Arc;

use rig::{Rig, alerts, pushed, settings};
use serde_json::{Value, json};
use wc_machine::{Effect, Event, Request, Response};
use wc_testkit::contract::{Contract, ResponseBody};

fn respond(rig: &mut Rig, request: Request) -> (Response, Vec<Effect>) {
    let effects = rig.send(Event::Edit { id: 7, request });
    let response = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::Respond { id: 7, response } => Some(response.clone()),
            _ => None,
        })
        .expect("every request is answered");
    (response, effects)
}

/// Answers a request and checks the status and body against the contract.
fn checked(rig: &mut Rig, method: &str, path: &str, request: Request) -> (Response, Vec<Effect>) {
    let (response, effects) = respond(rig, request);
    let body = ResponseBody::Json(response.body.clone());
    Contract::shared().validate_response(method, path, response.status, &body).unwrap();
    (response, effects)
}

fn put(rig: &mut Rig, section: &str, body: Value) -> (Response, Vec<Effect>) {
    let path = format!("/api/settings/{section}");
    checked(rig, "PUT", &path, Request::UpdateSettings { section: section.into(), body })
}

fn value(rig: &Rig, section: &str, key: &str) -> Value {
    rig.machine.snapshot().settings.value(section, key).cloned().unwrap()
}

fn field(response: &Response, key: &str) -> Value {
    response.body["settings"].as_array().unwrap().iter().find(|s| s["key"] == key).unwrap().clone()
}

fn saves(effects: &[Effect]) -> usize {
    effects.iter().filter(|e| matches!(e, Effect::SaveSettings(_))).count()
}

#[test]
fn an_update_saves_through_set_value_and_returns_the_fresh_section() {
    let mut rig = Rig::connected();
    let body = json!({"values": {"macro1_title": "Spindle", "zAxis": true, "zAxisSafeHeight": "7.5"}});
    let (response, effects) = put(&mut rig, "maslow", body);
    assert_eq!(response.status, 200);
    assert_eq!(value(&rig, "Maslow Settings", "macro1_title"), "Spindle");
    assert_eq!(value(&rig, "Maslow Settings", "zAxis"), 1);
    assert_eq!(value(&rig, "Maslow Settings", "zAxisSafeHeight"), "7.5");
    assert_eq!(field(&response, "macro1_title")["value"], "Spindle");
    assert_eq!(saves(&effects), 1);
    assert_eq!(rig.run_to_quiet(), ["$16=1.000000"]);
}

#[test]
fn an_update_leaves_settings_it_was_not_sent_alone() {
    let mut rig = Rig::connected();
    let before = rig.machine.snapshot().settings.clone();
    put(&mut rig, "advanced", json!({"values": {"truncate": true}}));
    let after = rig.machine.snapshot().settings.clone();
    let changed: Vec<String> = before
        .section("Advanced Settings")
        .iter()
        .zip(after.section("Advanced Settings"))
        .filter(|(old, new)| old["value"] != new["value"])
        .map(|(old, _)| old["key"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(changed, ["truncate"]);
}

#[test]
fn bools_turn_off_and_typed_numbers_keep_their_type() {
    let mut rig = Rig::connected();
    put(&mut rig, "maslow", json!({"values": {"bufferOn": true}}));
    put(&mut rig, "maslow", json!({"values": {"bufferOn": false}}));
    assert_eq!(value(&rig, "Maslow Settings", "bufferOn"), 0);
    put(&mut rig, "webcontrol", json!({"values": {"webPort": "5001", "loggingTimeout": "2.5"}}));
    assert_eq!(value(&rig, "WebControl Settings", "webPort"), 5001);
    assert_eq!(value(&rig, "WebControl Settings", "loggingTimeout"), 2.5);
}

#[test]
fn changing_a_firmware_setting_writes_it_to_the_controller() {
    let mut rig = Rig::connected();
    put(&mut rig, "maslow", json!({"values": {"motorSpacingX": "3000"}}));
    assert_eq!(rig.run_to_quiet(), ["$2=3000"]);
    put(&mut rig, "advanced", json!({"values": {"chainOverSprocket": "Top"}}));
    assert_eq!(rig.run_to_quiet(), ["$38=1.000000"]);
}

#[test]
fn a_blank_setting_saves_while_blank_and_refuses_text() {
    let mut rig = Rig::connected();
    let (described, _) = put(&mut rig, "advanced", json!({"values": {"truncate": true}}));
    assert_eq!(field(&described, "maxZRPM")["numeric"], true);
    let (refused, _) = put(&mut rig, "advanced", json!({"values": {"maxZRPM": "fast"}}));
    assert_eq!(refused.status, 400);
    assert!(rig.run_to_quiet().iter().all(|line| !line.starts_with("$18")));
}

#[test]
fn non_finite_numbers_never_reach_the_controller() {
    for key in ["motorSpacingX", "sledWeight"] {
        for text in ["nan", "1e999", "-1e400"] {
            let mut rig = Rig::connected();
            let before = value(&rig, "Maslow Settings", key);
            let (response, effects) = put(&mut rig, "maslow", json!({"values": {key: text}}));
            assert_eq!(response.body, json!({"errors": {key: "Must be a number"}}), "{key} {text}");
            assert_eq!(value(&rig, "Maslow Settings", key), before);
            assert_eq!(saves(&effects), 0);
            assert!(rig.run_to_quiet().is_empty());
        }
    }
}

#[test]
fn invalid_updates_are_refused_without_saving_anything() {
    let cases = [
        ("maslow", json!({"nope": "1"}), json!({"nope": "Unknown setting"})),
        (
            "maslow",
            json!({"motorSpacingX": "wide", "macro1_title": "Ok"}),
            json!({"motorSpacingX": "Must be a number"}),
        ),
        ("maslow", json!({"zAxis": "yes"}), json!({"zAxis": "Must be true or false"})),
        ("maslow", json!({"macro1": ["G0"]}), json!({"macro1": "Must be text or a number"})),
        (
            "advanced",
            json!({"chainOverSprocket": "Sideways"}),
            json!({"chainOverSprocket": "Must be one of the listed options"}),
        ),
        ("webcontrol", json!({"webPort": "50.5"}), json!({"webPort": "Must be a whole number"})),
        ("gpio", json!({"gpio4": "Explode"}), json!({"gpio4": "Must be one of the listed options"})),
        (
            "maslow",
            json!({"COMport": "/etc/passwd"}),
            json!({"COMport": "Must be a serial port such as /dev/ttyACM0 or COM3"}),
        ),
    ];
    for (section, values, errors) in cases {
        let mut rig = Rig::connected();
        let before = rig.machine.snapshot().settings.clone();
        let (response, effects) = respond(
            &mut rig,
            Request::UpdateSettings { section: section.into(), body: json!({"values": values.clone()}) },
        );
        assert_eq!((response.status, response.body), (400, json!({"errors": errors})), "{values}");
        assert_eq!(*rig.machine.snapshot().settings, *before);
        assert_eq!(saves(&effects), 0);
    }
}

#[test]
fn malformed_bodies_and_unknown_sections_are_refused() {
    let mut rig = Rig::connected();
    for body in [json!(null), json!([]), json!({"values": []}), json!({"other": {}})] {
        let (response, _) = put(&mut rig, "maslow", body);
        assert_eq!(response.status, 400);
        assert!(response.body["error"].is_string());
    }
    let (missing, _) =
        respond(&mut rig, Request::UpdateSettings { section: "nope".into(), body: json!({"values": {}}) });
    assert_eq!((missing.status, missing.body), (404, json!({"error": "Not found"})));
}

#[test]
fn custom_settings_are_locked_without_custom_firmware() {
    let custom = json!({"Maslow Settings": [{"key": "macro2", "value": "", "type": "string", "custom": 1}]});
    let home = tempfile::tempdir().unwrap();
    let paths = wc_config::Paths::new(home.path());
    std::fs::create_dir_all(paths.dir()).unwrap();
    std::fs::write(paths.settings_file(), custom.to_string()).unwrap();
    let mut rig = Rig::connected_with(wc_config::Settings::load(&paths));
    let (locked, _) = put(&mut rig, "maslow", json!({"values": {"macro2": "G0 X0"}}));
    assert!(locked.body["errors"]["macro2"].as_str().unwrap().starts_with("Needs custom firmware"));
    rig.rx("Firmware Version 100.5");
    let (saved, _) = put(&mut rig, "maslow", json!({"values": {"macro2": "G0 X0"}}));
    assert_eq!(saved.status, 200);
    assert_eq!(value(&rig, "Maslow Settings", "macro2"), "G0 X0");
}

#[test]
fn a_known_port_name_is_accepted_and_changing_it_reopens_the_link() {
    let mut rig = Rig::connected();
    rig.send(Event::Ports(vec!["/dev/weird port".into()]));
    let (response, effects) = put(&mut rig, "maslow", json!({"values": {"COMport": "/dev/weird port"}}));
    assert_eq!(response.status, 200);
    assert!(effects.contains(&Effect::ClosePort));
    assert_eq!(pushed(&effects, "connectionStatus").last().unwrap()["status"], "disconnected");
    let reopened = rig.advance(20);
    assert!(reopened.contains(&Effect::OpenPort("/dev/weird port".into())), "{reopened:?}");
    let (same, effects) = put(&mut rig, "maslow", json!({"values": {"COMport": "/dev/weird port"}}));
    assert_eq!(same.status, 200);
    assert!(!effects.contains(&Effect::ClosePort));
}

#[test]
fn ports_are_listed_on_the_maslow_section_and_gpio_offers_its_actions() {
    let mut rig = Rig::connected();
    rig.send(Event::Ports(vec!["/dev/ttyACM0".into()]));
    let (maslow, _) = put(&mut rig, "maslow", json!({"values": {}}));
    assert_eq!(maslow.body["ports"], json!(["/dev/ttyACM0"]));
    let (gpio, _) = put(&mut rig, "gpio", json!({"values": {"gpio4": "Stop"}}));
    assert_eq!(field(&gpio, "gpio4")["options"][4], "Stop");
    assert_eq!(value(&rig, "GPIO Settings", "gpio4"), "Stop");
    put(&mut rig, "gpio", json!({"values": {"gpio4": ""}}));
    assert_eq!(value(&rig, "GPIO Settings", "gpio4"), "");
}

const VALID_QUICK: &str = r#"{"kinematicsType": "Linkage", "rotationRadius": 273, "chainOverSprocket": "Top",
    "motorSpacingX": 2938.05, "motorOffsetY": 470}"#;

/// Ported from test_api_machine's endpoint round trip.
#[test]
fn quick_configure_saves_the_answers_and_reads_them_back() {
    let mut rig = Rig::connected();
    let body: Value = serde_json::from_str(VALID_QUICK).unwrap();
    let path = "/api/machine/quick-configure";
    let (response, _) = checked(&mut rig, "POST", path, Request::QuickConfigure(body));
    assert_eq!(
        response.body,
        json!({"kinematicsType": "Custom", "rotationRadius": 273.0, "chainOverSprocket": "Top",
            "motorSpacingX": 2938.05, "motorOffsetY": 470.0})
    );
    assert_eq!(value(&rig, "Advanced Settings", "kinematicsType"), "Triangular");
    assert_eq!(value(&rig, "Advanced Settings", "rotationRadius"), "273");
    assert_eq!(value(&rig, "Maslow Settings", "motorSpacingX"), "2938.05");
    assert_eq!(rig.run_to_quiet(), ["$8=273", "$38=1.000000", "$2=2938.05", "$3=470"]);
    let quad = json!({"kinematicsType": "Quadrilateral", "rotationRadius": 138.4, "chainOverSprocket": "Bottom",
        "motorSpacingX": 3000, "motorOffsetY": 470});
    let (quad, _) = checked(&mut rig, "POST", path, Request::QuickConfigure(quad));
    assert_eq!(quad.body["kinematicsType"], "Quadrilateral");
    assert_eq!(rig.run_to_quiet(), ["$7=1", "$8=138.4", "$38=2.00000", "$2=3000"]);
}

#[test]
fn quick_configure_refuses_an_invalid_body_and_changes_nothing() {
    let mut rig = Rig::connected();
    let before = rig.machine.snapshot().settings.clone();
    let mut body: Value = serde_json::from_str(VALID_QUICK).unwrap();
    body["chainOverSprocket"] = json!("Sideways");
    let (response, _) = checked(&mut rig, "POST", "/api/machine/quick-configure", Request::QuickConfigure(body));
    assert_eq!(response.status, 400);
    assert!(response.body["error"].as_str().unwrap().contains("chainOverSprocket"));
    assert_eq!(*rig.machine.snapshot().settings, *before);
}

fn pid_body(motor: &str) -> Value {
    json!({"kind": "velocity", "motor": motor, "start": 0, "stop": 10, "steps": 3, "version": 1, "kp": 1, "ki": 0,
        "kd": 0})
}

/// Ported from test_api_system's TestPid endpoint cases.
#[test]
fn a_pid_test_request_stores_the_gains_and_queues_b13() {
    let mut rig = Rig::connected();
    let (response, _) = checked(&mut rig, "POST", "/api/system/pid-test", Request::PidTest(pid_body("R")));
    assert_eq!((response.status, response.body), (202, json!({"status": "sent"})));
    assert_eq!(value(&rig, "Advanced Settings", "KpV"), "1");
    assert_eq!(rig.run_to_quiet(), ["B13 R1 S0 F10 I3 V1"]);
    let position = json!({"kind": "position", "motor": "N", "start": "0", "stop": "10", "steps": "2", "time": "3",
        "version": "2", "kp": "1400", "ki": "0", "kd": "34"});
    checked(&mut rig, "POST", "/api/system/pid-test", Request::PidTest(position));
    assert_eq!(value(&rig, "Advanced Settings", "KpPos"), "1400");
    assert_eq!(rig.run_to_quiet(), ["B14 N1 S0 F10 I2 T3 V2"]);
}

#[test]
fn a_pid_test_request_is_refused_when_invalid_or_while_a_job_runs() {
    let mut rig = Rig::connected();
    let (invalid, _) = checked(&mut rig, "POST", "/api/system/pid-test", Request::PidTest(json!({"kind": "velocity"})));
    assert_eq!(invalid.status, 400);
    assert!(invalid.body["error"].as_str().unwrap().contains("motor"));
    rig.machine.load_gcode("G0 X1\nG0 X2\n");
    rig.act("startRun");
    let (busy, _) = checked(&mut rig, "POST", "/api/system/pid-test", Request::PidTest(pid_body("L")));
    assert_eq!((busy.status, busy.body), (409, json!({"error": "Stop the job before starting a PID test."})));
}

#[test]
fn switching_units_converts_home_and_tells_every_client() {
    let mut rig = Rig::with(settings(&[
        ("Advanced Settings", "homeX", json!("254")),
        ("Advanced Settings", "homeY", json!("-25.4")),
    ]));
    rig.open();
    let effects = rig.send(Event::UpdateSetting { setting: "toInches".into(), value: json!("0.394") });
    assert_eq!(value(&rig, "Computed Settings", "units"), "INCHES");
    assert_eq!(value(&rig, "Computed Settings", "distToMove"), 0.394);
    assert_eq!(value(&rig, "Computed Settings", "tolerance"), 0.02);
    assert_eq!(value(&rig, "Advanced Settings", "homeX"), 10.0);
    assert_eq!(value(&rig, "Advanced Settings", "homeY"), -1.0);
    let settings: Vec<Value> = pushed(&effects, "requestedSetting");
    assert_eq!(
        settings,
        [json!({"setting": "units", "value": "INCHES"}), json!({"setting": "distToMove", "value": 0.394})]
    );
    assert_eq!(pushed(&effects, "homePositionMessage"), [json!({"xval": 10.0, "yval": -1.0})]);
    let effects = rig.send(Event::UpdateSetting { setting: "toMM".into(), value: json!(10) });
    assert_eq!(value(&rig, "Advanced Settings", "homeX"), 254.0);
    assert_eq!(value(&rig, "Computed Settings", "tolerance"), 0.5);
    assert_eq!(pushed(&effects, "homePositionMessage"), [json!({"xval": 254.0, "yval": -25.4})]);
}

#[test]
fn switching_units_sends_the_units_word_to_the_controller() {
    let mut rig = Rig::connected();
    rig.send(Event::UpdateSetting { setting: "toInches".into(), value: json!("1") });
    assert_eq!(rig.run_to_quiet(), ["G20"]);
    rig.send(Event::UpdateSetting { setting: "toMM".into(), value: json!("25.4") });
    assert_eq!(rig.run_to_quiet(), ["G21"]);
}

#[test]
fn the_z_panel_keeps_its_own_units() {
    let mut rig = Rig::connected();
    let effects = rig.send(Event::UpdateSetting { setting: "toInchesZ".into(), value: json!("0.04") });
    assert_eq!(value(&rig, "Computed Settings", "unitsZ"), "INCHES");
    assert_eq!(value(&rig, "Computed Settings", "units"), "MM");
    assert_eq!(
        pushed(&effects, "requestedSetting"),
        [json!({"setting": "unitsZ", "value": "INCHES"}), json!({"setting": "distToMoveZ", "value": 0.04})]
    );
    rig.send(Event::UpdateSetting { setting: "toMMZ".into(), value: json!(1) });
    assert_eq!(value(&rig, "Computed Settings", "unitsZ"), "MM");
    assert!(rig.run_to_quiet().is_empty());
}

#[test]
fn units_cannot_change_during_a_job_and_unknown_names_are_ignored() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode("G0 X1\nG0 X2\n");
    rig.act("startRun");
    let refused = rig.send(Event::UpdateSetting { setting: "toMMZ".into(), value: json!("1") });
    assert_eq!(alerts(&refused), [wc_machine::files::SETTINGS_LOCKED], "decision 2, where Python said 'units'");
    let mut idle = Rig::connected();
    assert!(alerts(&idle.send(Event::UpdateSetting { setting: "other".into(), value: json!("1") })).is_empty());
}

#[test]
fn an_unreadable_home_fails_the_switch_with_an_alert() {
    let mut rig = Rig::with(settings(&[("Advanced Settings", "homeX", json!("left"))]));
    let effects = rig.send(Event::UpdateSetting { setting: "toInches".into(), value: json!("1") });
    assert_eq!(alerts(&effects), ["Error updating setting"]);
}

#[test]
fn every_change_is_handed_to_the_writer_once_and_nothing_else_is() {
    let mut rig = Rig::connected();
    assert_eq!(saves(&rig.advance(100)), 0);
    let effects = rig.send(Event::UpdateSetting { setting: "toMMZ".into(), value: json!("1") });
    let saved: Vec<Arc<wc_config::Settings>> = effects
        .iter()
        .filter_map(|e| match e {
            Effect::SaveSettings(s) => Some(s.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].value("Computed Settings", "distToMoveZ"), Some(&json!(1.0)));
    assert_eq!(saves(&rig.advance(100)), 0);
}

#[test]
fn a_value_adopted_from_the_controller_is_saved() {
    let mut rig = Rig::connected();
    let effects = rig.rx("$18=12.60000000");
    assert_eq!(value(&rig, "Advanced Settings", "maxZRPM"), "12.6");
    assert_eq!(saves(&effects), 1);
}
