use serde_json::json;
use wc_testkit::contract::{Contract, ResponseBody};

fn contract() -> &'static Contract {
    Contract::shared()
}

#[test]
fn the_version_is_the_one_in_contract_version() {
    let text = std::fs::read_to_string(wc_testkit::repo_root().join("contract/VERSION")).unwrap();
    assert_eq!(contract().version(), text.trim().parse::<u32>().unwrap());
}

#[test]
fn a_message_must_have_its_required_fields() {
    assert!(contract().validate_message("hostAddress", &json!({"hostAddress": "10.0.0.5:5000"})).is_ok());
    let err = contract().validate_message("hostAddress", &json!({})).unwrap_err();
    assert!(err.contains("hostAddress"), "{err}");
}

#[test]
fn a_field_the_schema_does_not_list_fails() {
    let err = contract().validate_message("hostAddress", &json!({"hostAddress": "x", "extra": 1})).unwrap_err();
    assert!(err.contains("extra"), "{err}");
}

#[test]
fn a_schema_that_allows_extra_fields_stays_open() {
    let alert = json!({"title": "Alert", "message": "m", "somethingElse": true});
    assert!(contract().validate_message("activateModal", &alert).is_ok());
}

#[test]
fn an_unknown_command_fails() {
    assert!(contract().validate_message("noSuchCommand", &json!({})).unwrap_err().contains("noSuchCommand"));
}

#[test]
fn a_json_envelope_carries_its_data_as_json_text() {
    let ok = json!({"command": "hostAddress", "data": "{\"hostAddress\": \"h\"}", "dataFormat": "json"});
    assert!(contract().validate_envelope(&ok).is_ok());
    let object = json!({"command": "hostAddress", "data": {"hostAddress": "h"}, "dataFormat": "json"});
    assert!(contract().validate_envelope(&object).unwrap_err().contains("JSON text"));
    let garbage = json!({"command": "hostAddress", "data": "{nope", "dataFormat": "json"});
    assert!(contract().validate_envelope(&garbage).unwrap_err().contains("JSON text"));
}

#[test]
fn an_envelope_must_use_the_commands_data_format() {
    let wrong = json!({"command": "hostAddress", "data": "{}", "dataFormat": "int"});
    assert!(contract().validate_envelope(&wrong).unwrap_err().contains("dataFormat"));
    let missing = json!({"command": "showFPSpinner", "data": 1});
    assert!(contract().validate_envelope(&missing).unwrap_err().contains("dataFormat"));
}

#[test]
fn int_and_base64_envelopes_validate_their_raw_data() {
    let spinner = json!({"command": "showFPSpinner", "data": 1, "dataFormat": "int"});
    assert!(contract().validate_envelope(&spinner).is_ok());
    let nothing = json!({"command": "gcodeUpdateCompressed", "data": "", "dataFormat": "base64"});
    assert!(contract().validate_envelope(&nothing).is_ok());
}

#[test]
fn an_envelope_with_extra_fields_fails() {
    let extra = json!({"command": "showFPSpinner", "data": 1, "dataFormat": "int", "x": 1});
    assert!(contract().validate_envelope(&extra).is_err());
}

#[test]
fn the_connect_handshake_frame_carries_only_a_sid() {
    assert!(contract().validate_frame(r#"{"ns":"/MaslowCNC","event":"connect","data":{"sid":"abc"}}"#).is_ok());
    assert!(contract().validate_frame(r#"{"ns":"/MaslowCNC","event":"connect","data":{}}"#).is_err());
    assert!(contract().validate_frame(r#"{"ns":"/MaslowCNC","event":"connect","data":{"sid":"a","x":1}}"#).is_err());
}

#[test]
fn a_message_frame_validates_its_envelope() {
    let frame = json!({"ns": "/MaslowCNC", "event": "message",
        "data": {"command": "hostAddress", "data": "{\"hostAddress\": 3}", "dataFormat": "json"}});
    assert!(contract().validate_frame(&frame.to_string()).is_err());
}

#[test]
fn a_log_frame_carries_the_log_object_itself() {
    let log = json!({"ns": "/MaslowCNCLogs", "event": "message", "data": {"log": "state", "state": true}});
    assert!(contract().validate_frame(&log.to_string()).is_ok());
    let bad = json!({"ns": "/MaslowCNCLogs", "event": "message", "data": {"log": "nope"}});
    assert!(contract().validate_frame(&bad.to_string()).is_err());
}

#[test]
fn client_event_frames_validate_their_payload() {
    let action =
        json!({"ns": "/MaslowCNC", "event": "action", "data": {"data": {"command": "home", "arg": "", "arg1": ""}}});
    assert!(contract().validate_frame(&action.to_string()).is_ok());
    let bad = json!({"ns": "/MaslowCNC", "event": "action", "data": {"data": {"command": "home"}}});
    assert!(contract().validate_frame(&bad.to_string()).is_err());
}

#[test]
fn frames_that_are_not_contract_frames_fail() {
    assert!(contract().validate_frame("not json").is_err());
    assert!(contract().validate_frame(r#"{"ns":"/MaslowCNC","event":"message"}"#).is_err());
    assert!(contract().validate_frame(r#"{"ns":"/Nope","event":"message","data":1}"#).unwrap_err().contains("/Nope"));
    let unknown = r#"{"ns":"/MaslowCNC","event":"noSuchEvent","data":1}"#;
    assert!(contract().validate_frame(unknown).unwrap_err().contains("noSuchEvent"));
}

#[test]
fn every_validated_command_is_recorded() {
    let contract = Contract::load();
    contract.validate_message("hostAddress", &json!({"hostAddress": "h"})).unwrap();
    let _ = contract.validate_message("connectionStatus", &json!({}));
    assert_eq!(contract.seen_commands(), ["hostAddress"].map(String::from).into());
}

#[test]
fn responses_validate_against_the_endpoint_and_status() {
    let state = json!({"units": "MM", "distToMove": 1, "unitsZ": "MM", "distToMoveZ": 0.1,
        "homePosition": {"x": 0, "y": 0}, "pauseButtonSetting": "Pause", "hostAddress": "-", "macros": []});
    assert!(contract().validate_response("GET", "/api/state", 200, &ResponseBody::Json(state.clone())).is_ok());
    let mut extra = state.clone();
    extra["surprise"] = json!(1);
    assert!(contract().validate_response("GET", "/api/state", 200, &ResponseBody::Json(extra)).is_err());
    let err = contract().validate_response("GET", "/api/state", 500, &ResponseBody::Json(state)).unwrap_err();
    assert!(err.contains("500"), "{err}");
}

#[test]
fn path_parameters_match_templates() {
    let entry = contract().endpoint("GET", "/api/settings/maslow").unwrap();
    assert_eq!(entry.path, "/api/settings/{sectionId}");
    assert!(contract().endpoint("GET", "/api/settings/a/b").is_none());
    assert!(contract().endpoint("DELETE", "/api/state").is_none());
    assert_eq!(contract().endpoint("GET", "/next/a/b/c.js").unwrap().path, "/next/{path}");
}

#[test]
fn non_json_responses_check_their_content_type() {
    let html = ResponseBody::Other("text/html; charset=utf-8".into());
    assert!(contract().validate_response("GET", "/api/settings/x", 404, &html).is_ok());
    let text = ResponseBody::Other("text/plain".into());
    assert!(contract().validate_response("GET", "/api/settings/x", 404, &text).unwrap_err().contains("text/html"));
    let octets = ResponseBody::Other("image/svg+xml".into());
    assert!(contract().validate_response("GET", "/static/images/a.svg", 200, &octets).is_ok());
    let json = ResponseBody::Json(json!({}));
    assert!(contract().validate_response("GET", "/api/settings/x", 404, &json).unwrap_err().contains("text/html"));
    assert!(contract().validate_response("GET", "/nowhere", 200, &json).unwrap_err().contains("/nowhere"));
}

#[test]
fn request_bodies_validate_against_the_request_schema() {
    let path = "/api/files/gcode/directories";
    assert!(contract().validate_request("POST", path, &json!({"name": "jobs"})).is_ok());
    assert!(contract().validate_request("POST", path, &json!({"name": "jobs", "parent": ""})).is_err());
    assert!(contract().validate_request("GET", "/api/state", &json!({})).unwrap_err().contains("no request body"));
    assert!(contract().validate_request("GET", "/nowhere", &json!({})).is_err());
}

#[test]
fn lists_every_endpoint_event_and_action() {
    let contract = contract();
    assert!(contract.endpoints().iter().any(|e| e.method == "GET" && e.path == "/api/state"));
    assert!(contract.client_events("/MaslowCNC").contains(&"action".to_string()));
    assert!(contract.client_events("/MaslowCNCLogs").is_empty());
    assert!(contract.message_commands().contains(&"hostAddress".to_string()));
    assert!(contract.actions().contains_key("stopRun"));
}
