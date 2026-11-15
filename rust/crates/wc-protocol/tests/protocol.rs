use serde_json::{Value, json};
use wc_protocol::main::{
    ActivateModal, Alarm, Backend, ClearAlarm, ConnectionStatus, ControllerMessage, ErrorValueMessage, GcodeLine,
    HealthMessage, HomePositionMessage, HostAddress, Macro, MainScreen, PauseButton, PositionMessage, RequestedSetting,
    StartupState, StatusMessage, Units,
};
use wc_protocol::settings::UpdatePorts;
use wc_protocol::{Action, ClientEvent, Envelope, Frame, Namespace, Push, float};
use wc_testkit::contract::{Contract, ResponseBody};

fn contract() -> &'static Contract {
    Contract::shared()
}

fn startup_state() -> StartupState {
    StartupState {
        units: Units::Mm,
        dist_to_move: 1.0,
        units_z: Units::Inches,
        dist_to_move_z: 0.1,
        home_position: wc_protocol::main::Point { x: 12.5, y: -3.0 },
        pause_button_setting: PauseButton::Pause,
        job_in_progress: false,
        manual_z_axis_adjust: false,
        host_address: "10.0.0.5:5000".into(),
        controller_firmware_version: 0.0,
        controller_board: None,
        firmware_build: None,
        macros: vec![Macro::new(1, "Spindle on"), Macro::new(2, "")],
        backend: Backend::Rust,
    }
}

#[test]
fn the_contract_version_is_read_from_contract_version() {
    assert_eq!(wc_protocol::CONTRACT_VERSION, contract().version());
}

#[test]
fn json_envelopes_double_encode_their_data() {
    let envelope = Envelope::json(&HostAddress { host_address: "10.0.0.5:5000".into() });
    let value = serde_json::to_value(&envelope).unwrap();
    assert_eq!(value["command"], "hostAddress");
    assert_eq!(value["dataFormat"], "json");
    let text = value["data"].as_str().expect("data is JSON text");
    assert_eq!(serde_json::from_str::<Value>(text).unwrap(), json!({"hostAddress": "10.0.0.5:5000"}));
    contract().validate_envelope(&value).unwrap();
}

#[test]
fn int_envelopes_carry_a_number() {
    let value = serde_json::to_value(Envelope::int("showFPSpinner", 1)).unwrap();
    assert_eq!(value, json!({"command": "showFPSpinner", "data": 1, "dataFormat": "int"}));
    contract().validate_envelope(&value).unwrap();
}

#[test]
fn base64_envelopes_carry_text_or_nothing() {
    let some = serde_json::to_value(Envelope::base64("gcodeUpdateCompressed", Some(b"\x1f\x8b"))).unwrap();
    assert_eq!(some["data"], "H4s=");
    contract().validate_envelope(&some).unwrap();
    let none = serde_json::to_value(Envelope::base64("boardCutDataUpdateCompressed", None)).unwrap();
    assert_eq!(none["data"], "");
    contract().validate_envelope(&none).unwrap();
}

#[test]
fn a_disconnected_controller_reports_port_none() {
    let envelope = serde_json::to_value(Envelope::json(&ConnectionStatus::disconnected())).unwrap();
    contract().validate_envelope(&envelope).unwrap();
    let data: Value = serde_json::from_str(envelope["data"].as_str().unwrap()).unwrap();
    assert_eq!(data, json!({"status": "disconnected", "port": "none", "fakeServoStatus": false}));
}

#[test]
fn alerts_match_pythons_activate_modal() {
    let envelope = serde_json::to_value(Envelope::json(&ActivateModal::alert("Cannot do that"))).unwrap();
    contract().validate_envelope(&envelope).unwrap();
    let data: Value = serde_json::from_str(envelope["data"].as_str().unwrap()).unwrap();
    let python = json!({"title": "Alert", "message": "Cannot do that", "resume": "false", "progress": "false",
        "modalSize": "small", "modalType": "alert"});
    assert_eq!(data, python);
}

#[test]
fn the_startup_state_fits_get_api_state() {
    let body = serde_json::to_value(startup_state()).unwrap();
    contract().validate_response("GET", "/api/state", 200, &ResponseBody::Json(body.clone())).unwrap();
    assert_eq!(body["backend"], "rust");
    assert_eq!(body["unitsZ"], "INCHES");
    assert_eq!(body["controllerBoard"], Value::Null);
    assert_eq!(body["macros"][0], json!({"command": "macro1", "title": "Spindle on"}));
}

#[test]
fn pause_and_resume_labels_and_units_spell_like_python() {
    let state = StartupState { pause_button_setting: PauseButton::Resume, ..startup_state() };
    assert_eq!(serde_json::to_value(state).unwrap()["pauseButtonSetting"], "Resume");
    assert_eq!(Units::from_setting("MM"), Units::Mm);
    assert_eq!(Units::from_setting("INCHES"), Units::Inches);
    assert_eq!(Units::from_setting("furlongs"), Units::Inches);
    assert_eq!(serde_json::to_value(Backend::Python).unwrap(), "python");
}

#[test]
fn namespaces_parse_from_their_paths() {
    assert_eq!("/MaslowCNC".parse::<Namespace>(), Ok(Namespace::MaslowCnc));
    assert_eq!("/MaslowCNCLogs".parse::<Namespace>(), Ok(Namespace::Logs));
    assert!("/WebMCP".parse::<Namespace>().is_err());
    assert_eq!(Namespace::Logs.path(), "/MaslowCNCLogs");
}

#[test]
fn the_connect_frame_is_the_handshake() {
    let text = Frame::connect(Namespace::MaslowCnc, "abc").encode();
    contract().validate_frame(&text).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&text).unwrap(),
        json!({"ns": "/MaslowCNC", "event": "connect", "data": {"sid": "abc"}})
    );
}

#[test]
fn frames_round_trip_and_garbage_is_ignored() {
    let frame = Frame { ns: "/MaslowCNC".into(), event: "action".into(), data: json!({"data": {}}) };
    assert_eq!(Frame::decode(&frame.encode()), Some(frame));
    assert_eq!(Frame::decode("not json"), None);
    assert_eq!(Frame::decode(r#"{"ns":"/MaslowCNC","event":"x"}"#), None);
    assert_eq!(Frame::decode(r#"["/MaslowCNC","message"]"#), None);
}

#[test]
fn pushes_become_message_frames_for_their_namespace() {
    let push = Push::message(Envelope::json(&HostAddress { host_address: "h".into() }));
    assert_eq!(push.client, None);
    let text = push.frame().encode();
    contract().validate_frame(&text).unwrap();
    let to_one = Push::message(Envelope::int("showFPSpinner", 0)).to_client("sid1");
    assert_eq!(to_one.client.as_deref(), Some("sid1"));
    assert_eq!(to_one.ns, Namespace::MaslowCnc);
    let log = Push::log(json!({"log": "state", "state": false}));
    contract().validate_frame(&log.frame().encode()).unwrap();
}

#[test]
fn action_events_decode_with_lenient_arguments() {
    let frame = |data: Value| Frame { ns: "/MaslowCNC".into(), event: "action".into(), data };
    let full = frame(json!({"data": {"command": "move", "arg": "up", "arg1": 10}}));
    let expected = Action { command: "move".into(), arg: json!("up"), arg1: json!(10) };
    assert_eq!(ClientEvent::from_frame(&full), Some(ClientEvent::Action(expected)));
    let bare = frame(json!({"data": {"command": "home"}}));
    let defaults = Action { command: "home".into(), arg: json!(""), arg1: json!("") };
    assert_eq!(ClientEvent::from_frame(&bare), Some(ClientEvent::Action(defaults)));
    assert_eq!(ClientEvent::from_frame(&frame(json!({"data": {"arg": 1}}))), None);
    assert_eq!(ClientEvent::from_frame(&frame(json!("nope"))), None);
}

#[test]
fn setting_requests_decode_and_other_events_are_named_but_not_decoded_yet() {
    let frame = |event: &str, data: Value| Frame { ns: "/MaslowCNC".into(), event: event.into(), data };
    let request = frame("settingRequest", json!({"data": {"section": "Computed Settings", "setting": "units"}}));
    contract().validate_frame(&request.encode()).unwrap();
    assert_eq!(
        ClientEvent::from_frame(&request),
        Some(ClientEvent::SettingRequest { section: "Computed Settings".into(), setting: "units".into() })
    );
    assert_eq!(ClientEvent::from_frame(&frame("settingRequest", json!({"data": {"section": 1}}))), None);
    assert_eq!(ClientEvent::from_frame(&frame("settingRequest", json!({}))), None);
    let later = frame("opticalCalibration", json!({}));
    assert_eq!(ClientEvent::from_frame(&later), Some(ClientEvent::Unhandled("opticalCalibration".into())));
    assert_eq!(ClientEvent::from_frame(&frame("bogus", json!({}))), None);
}

#[test]
fn action_arguments_read_like_pythons_float() {
    assert_eq!(float(&json!(10)), Some(10.0));
    assert_eq!(float(&json!(" 2.5 ")), Some(2.5));
    assert_eq!(float(&json!("1e3")), Some(1000.0));
    [json!(""), json!("nan"), json!("inf"), json!("up"), json!(null), json!(true)]
        .iter()
        .for_each(|value| assert_eq!(float(value), None, "{value}"));
    assert_eq!(Action::new("move", json!("up"), json!(1)).arg_text(), "up");
    assert_eq!(Action::new("defineHome", json!(10), json!(1)).arg_text(), "10");
}

/// Every message type the link sends, checked against its closed schema.
#[test]
fn link_messages_fit_their_schemas() {
    let check = |envelope: Envelope| contract().validate_envelope(&serde_json::to_value(envelope).unwrap()).unwrap();
    check(Envelope::json(&PositionMessage {
        xval: 1.0,
        yval: 2.0,
        zval: 0.5,
        vel: 0.0,
        rqvel: 20.0,
        pcom: "0.0%".into(),
        state: "Idle".into(),
        lpwr: 15.0,
        rpwr: 15.0,
        zpwr: 15.0,
    }));
    check(Envelope::json(&ErrorValueMessage {
        left_error: 0.1,
        right_error: 0.2,
        computed_x: -999999.0,
        computed_y: -999999.0,
        computed_enabled: 0,
    }));
    check(Envelope::json(&HealthMessage { cpu_usage: 12.5, buffer_size: -1, controller_firmware_version: 51.27 }));
    let status = StatusMessage {
        upload_flag: 0,
        positioning_mode: 0,
        current_tool: 0,
        job_in_progress: false,
        manual_z_axis_adjust: false,
    };
    check(Envelope::json(&status));
    check(Envelope::json(&ControllerMessage("Firmware Version 51.27".into())));
    check(Envelope::json(&Alarm { message: "Alarm: Sled Not Keeping Up".into() }));
    check(Envelope::json(&ClearAlarm::default()));
    check(Envelope::json(&HomePositionMessage { xval: 1.0, yval: -2.0 }));
    check(Envelope::json(&RequestedSetting { setting: "units".into(), value: json!("MM") }));
    check(Envelope::json(&UpdatePorts(vec!["None".into()])));
    check(Envelope::json(&ActivateModal::titled_alert("Touch Z Failed", "Z was not zeroed")));
    let notification = ActivateModal::notification("Please insert tool 2", true);
    assert_eq!((notification.title.as_str(), notification.resume.as_str()), ("Notification:", "resume"));
    assert_eq!(notification.modal_type, "notification");
    check(Envelope::json(&notification));
    assert_eq!(ActivateModal::notification("x", false).resume, "false");
    let main = MainScreen {
        status,
        touch_plate: false,
        gcode: GcodeLine { line_index: 0, line: String::new(), line_count: 0 },
    };
    contract().check("schemas/http/get-api-main.200.json", &serde_json::to_value(main).unwrap()).unwrap();
}

#[test]
fn the_stops_are_the_realtime_actions() {
    let action = |command: &str| Action { command: command.into(), arg: json!(""), arg1: json!("") };
    assert!(action("stopRun").is_realtime());
    assert!(action("stopZ").is_realtime());
    assert!(!action("move").is_realtime());
}
