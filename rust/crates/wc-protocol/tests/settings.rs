//! M2's wire types: the unit switch event, the setup screens' bodies and the PID test, against the closed schemas.

use serde_json::{Value, json};
use wc_protocol::machine::{
    MachineSetup, PidKind, PidResult, QuickConfigure, QuickConfigureForm, SettingsUpdate, UpdatePidData,
};
use wc_protocol::system::{PidGains, PidSettings, PidTest};
use wc_protocol::{ClientEvent, Envelope, Frame};
use wc_testkit::contract::Contract;

fn contract() -> &'static Contract {
    Contract::shared()
}

fn frame(event: &str, data: Value) -> Frame {
    Frame { ns: "/MaslowCNC".into(), event: event.into(), data }
}

#[test]
fn the_unit_switch_event_decodes_text_or_numbers() {
    let switch = frame("updateSetting", json!({"data": {"setting": "toInches", "value": "0.394"}}));
    contract().validate_frame(&switch.encode()).unwrap();
    assert_eq!(
        ClientEvent::from_frame(&switch),
        Some(ClientEvent::UpdateSetting { setting: "toInches".into(), value: json!("0.394") })
    );
    let number = frame("updateSetting", json!({"data": {"setting": "toMMZ", "value": 25.4}}));
    contract().validate_frame(&number.encode()).unwrap();
    assert!(matches!(ClientEvent::from_frame(&number), Some(ClientEvent::UpdateSetting { .. })));
    [
        json!({}),
        json!({"data": {"setting": "toMM"}}),
        json!({"data": {"setting": 1, "value": "1"}}),
        json!({"data": {"setting": "toMM", "value": [1]}}),
    ]
    .into_iter()
    .for_each(|data| assert_eq!(ClientEvent::from_frame(&frame("updateSetting", data.clone())), None, "{data}"));
}

#[test]
fn the_setup_and_quick_configure_answers_fit_their_schemas() {
    let setup = MachineSetup {
        chain_extend_length: 1651.0,
        four_motor: false,
        firmware_supports_zaxis_limit: false,
        touch_plate: true,
        controller_firmware_version: 51.27,
        firmware_build: Some("51.27".into()),
    };
    contract().check("schemas/http/get-api-machine-setup.200.json", &serde_json::to_value(setup).unwrap()).unwrap();
    let quick = QuickConfigure {
        kinematics_type: "Ring".into(),
        rotation_radius: 138.4,
        chain_over_sprocket: "Top".into(),
        motor_spacing_x: 2978.4,
        motor_offset_y: 463.0,
    };
    let value = serde_json::to_value(quick).unwrap();
    contract().check("schemas/http/get-api-machine-quick-configure.200.json", &value).unwrap();
    contract().check("schemas/http/post-api-machine-quick-configure.200.json", &value).unwrap();
}

const VALID_QUICK: &str = r#"{"kinematicsType": "Linkage", "rotationRadius": 273, "chainOverSprocket": "Top",
    "motorSpacingX": 2938.05, "motorOffsetY": 470}"#;

/// Ported from test_api_machine's parse tests.
#[test]
fn a_quick_configure_body_is_checked_like_api_machine() {
    let valid: Value = serde_json::from_str(VALID_QUICK).unwrap();
    contract().validate_request("POST", "/api/machine/quick-configure", &valid).unwrap();
    assert_eq!(
        QuickConfigureForm::parse(&valid),
        Ok(QuickConfigureForm {
            kinematics_type: "Linkage".into(),
            chain_over_sprocket: "Top".into(),
            rotation_radius: 273.0,
            motor_spacing_x: 2938.05,
            motor_offset_y: 470.0,
        })
    );
    let with = |key: &str, value: Value| {
        let mut body = valid.clone();
        body[key] = value;
        body
    };
    let bad = [
        (json!(null), "JSON object"),
        (json!([]), "JSON object"),
        (with("kinematicsType", json!("Triangle")), "kinematicsType"),
        (with("chainOverSprocket", json!("Left")), "chainOverSprocket"),
        (with("rotationRadius", json!("273")), "rotationRadius"),
        (with("motorSpacingX", json!(true)), "motorSpacingX"),
        (with("motorOffsetY", json!(-1)), "motorOffsetY"),
    ];
    bad.iter().for_each(|(body, message)| {
        let error = QuickConfigureForm::parse(body).unwrap_err();
        assert!(error.contains(message), "{body}: {error}");
        assert!(contract().validate_request("POST", "/api/machine/quick-configure", body).is_err(), "{body}");
    });
}

#[test]
fn a_settings_update_needs_its_values_object() {
    let body = json!({"values": {"macro1_title": "Spindle", "zAxis": true}});
    contract().validate_request("PUT", "/api/settings/maslow", &body).unwrap();
    assert_eq!(SettingsUpdate::parse(&body).unwrap().values.len(), 2);
    [json!(null), json!([]), json!({"values": []}), json!({"other": {}})].iter().for_each(|body| {
        assert_eq!(SettingsUpdate::parse(body), Err(SettingsUpdate::MALFORMED.to_string()), "{body}");
        assert!(contract().validate_request("PUT", "/api/settings/maslow", body).is_err(), "{body}");
    });
}

fn velocity(kp: Value) -> Value {
    json!({"kind": "velocity", "motor": "L", "start": "0", "stop": "1", "steps": "1", "version": "1",
        "kp": kp, "ki": "0", "kd": "0"})
}

/// Ported from test_api_system's TestPid parse cases.
#[test]
fn a_pid_test_body_is_checked_like_api_system() {
    let body = velocity(json!("1300"));
    contract().validate_request("POST", "/api/system/pid-test", &body).unwrap();
    let test = PidTest::parse(&body).unwrap();
    assert_eq!((test.kind, test.motor.as_str()), (PidKind::Velocity, "L"));
    assert_eq!(test.fields[6], ("kd", "0".to_string()));
    let position = json!({"kind": "position", "motor": "N", "start": 0, "stop": 10, "steps": 2, "time": 3,
        "version": 1, "kp": 1, "ki": 2.5, "kd": 3.0});
    let fields: Vec<String> = PidTest::parse(&position).unwrap().fields.into_iter().map(|(_, t)| t).collect();
    assert_eq!(fields, ["0", "10", "2", "3", "1", "1", "2.5", "3.0"]);
    let bad = [
        (json!({"kind": "speed"}), "kind"),
        (json!({"kind": "velocity", "motor": "Z"}), "motor"),
        (json!({"kind": "position", "motor": "L", "start": "x"}), "not numbers: start, stop, steps, time"),
    ];
    bad.iter().for_each(|(body, message)| {
        let error = PidTest::parse(body).unwrap_err();
        assert!(error.contains(message), "{body}: {error}");
    });
    [
        json!("nan"),
        json!("inf"),
        json!("-inf"),
        json!("1e400"),
        json!("1_0"),
        json!(true),
        json!(" "),
        json!(null),
        json!(1e20),
        json!(1e-7),
    ]
    .into_iter()
    .for_each(|kp| assert_eq!(PidTest::parse(&velocity(kp.clone())).unwrap_err(), "not numbers: kp", "{kp}"));
    [
        (json!("-1.5"), "-1.5"),
        (json!(".5"), ".5"),
        (json!("2."), "2."),
        (json!(" 3 "), "3"),
        (json!(4), "4"),
        (json!(0.25), "0.25"),
    ]
    .into_iter()
    .for_each(|(kp, text)| assert_eq!(PidTest::parse(&velocity(kp)).unwrap().fields[4].1, text));
}

#[test]
fn pid_answers_and_results_fit_their_schemas() {
    let gains = |kp: &str| PidGains { kp: kp.into(), ki: "0".into(), kd: "0.28".into() };
    let settings = PidSettings { velocity: gains("5"), position: gains("1300"), four_motor: false };
    contract().check("schemas/http/get-api-system-pid.200.json", &serde_json::to_value(settings).unwrap()).unwrap();
    let results = [
        PidResult { result: PidKind::Velocity, version: json!("1"), data: vec![json!(1.5), json!(2.0)] },
        PidResult { result: PidKind::Position, version: json!("2"), data: vec![json!("12.0,11.5,30")] },
    ];
    results.into_iter().for_each(|result| {
        let envelope = serde_json::to_value(Envelope::json(&UpdatePidData::new(result))).unwrap();
        contract().validate_envelope(&envelope).unwrap();
    });
}
