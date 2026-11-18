//! M2's routes over HTTP: the settings save, the setup screens and the PID test, each body checked against its
//! closed schema; and the save reaching webcontrol.json through the host.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tokio::sync::Notify;
use wc_config::{Paths, Settings};
use wc_machine::{Event, Host, Machine, NoHost, NoPort, spawn};
use wc_server::host::SystemHost;
use wc_server::{AppState, PingConfig, router};
use wc_testkit::contract::{Contract, ResponseBody};

async fn call(machine: Machine, host: Arc<dyn Host>, method: &str, path: &str, body: Option<&str>) -> (u16, Value) {
    let (machine, task) = spawn(machine, Box::new(NoPort), host);
    let app =
        router(AppState::new(machine, PingConfig::default(), Paths::new(std::path::Path::new("/nonexistent-home"))));
    let request = Request::builder().method(method).uri(path).header(header::CONTENT_TYPE, "application/json");
    let request = request.body(body.map_or_else(Body::empty, |b| Body::from(b.to_string()))).unwrap();
    let response = tower::ServiceExt::oneshot(app, request).await.unwrap();
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    // A 404 is JSON where the contract records Flask's HTML page (wart 9).
    if status != 404 {
        Contract::shared().validate_response(method, path, status, &ResponseBody::Json(value.clone())).unwrap();
    }
    if let Some(text) = body.filter(|_| status < 400) {
        Contract::shared().validate_request(method, path, &serde_json::from_str(text).unwrap()).unwrap();
    }
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), task).await;
    (status, value)
}

async fn fresh(method: &str, path: &str, body: Option<&str>) -> (u16, Value) {
    call(Machine::new("h", Settings::defaults()), Arc::new(NoHost), method, path, body).await
}

#[tokio::test]
async fn a_settings_save_answers_with_the_saved_section() {
    let (status, body) = fresh("PUT", "/api/settings/maslow", Some(r#"{"values": {"macro1_title": "Spindle"}}"#)).await;
    assert_eq!(status, 200);
    let title = body["settings"].as_array().unwrap().iter().find(|s| s["key"] == "macro1_title").unwrap().clone();
    assert_eq!(title["value"], "Spindle");
    assert!(title["desc"].as_str().unwrap().ends_with("default setting: Macro 1"));
}

#[tokio::test]
async fn a_settings_save_refuses_bad_bodies_and_unknown_sections() {
    assert_eq!(fresh("PUT", "/api/settings/maslow", Some("not json")).await.0, 400);
    let (status, body) = fresh("PUT", "/api/settings/maslow", Some(r#"{"values": {"zAxis": "yes"}}"#)).await;
    assert_eq!((status, body), (400, json!({"errors": {"zAxis": "Must be true or false"}})));
    let (status, body) = fresh("PUT", "/api/settings/nope", Some(r#"{"values": {}}"#)).await;
    assert_eq!((status, body), (404, json!({"error": "Not found"})));
}

#[tokio::test]
async fn the_setup_screens_read_the_live_settings() {
    let (status, setup) = fresh("GET", "/api/machine/setup", None).await;
    assert_eq!(status, 200);
    assert_eq!(
        setup,
        json!({"chainExtendLength": 1651.0, "fourMotor": false, "firmwareSupportsZaxisLimit": false,
            "touchPlate": false, "controllerFirmwareVersion": 0.0, "firmwareBuild": null})
    );
    let (_, quick) = fresh("GET", "/api/machine/quick-configure", None).await;
    assert_eq!(
        quick,
        json!({"kinematicsType": "Custom", "rotationRadius": 139.1, "chainOverSprocket": "Bottom",
            "motorSpacingX": 2978.4, "motorOffsetY": 463.0})
    );
    let (_, pid) = fresh("GET", "/api/system/pid", None).await;
    assert_eq!(
        pid,
        json!({"velocity": {"kp": "5", "ki": "0", "kd": "0.28"}, "position": {"kp": "1300", "ki": "0", "kd": "34"},
            "fourMotor": false})
    );
}

#[tokio::test]
async fn quick_configure_saves_and_refuses_over_http() {
    let body = r#"{"kinematicsType": "Ring", "rotationRadius": 138.4, "chainOverSprocket": "Top",
        "motorSpacingX": 3000, "motorOffsetY": 470}"#;
    let (status, saved) = fresh("POST", "/api/machine/quick-configure", Some(body)).await;
    assert_eq!((status, saved["kinematicsType"].clone()), (200, json!("Ring")));
    let (status, refused) = fresh("POST", "/api/machine/quick-configure", Some("[]")).await;
    assert_eq!((status, refused), (400, json!({"error": "Expected a JSON object"})));
}

#[tokio::test]
async fn a_pid_test_is_accepted_and_a_bad_one_refused() {
    let body = r#"{"kind": "velocity", "motor": "L", "start": "0", "stop": "1", "steps": "1", "version": "1",
        "kp": "1", "ki": "0", "kd": "0"}"#;
    assert_eq!(fresh("POST", "/api/system/pid-test", Some(body)).await, (202, json!({"status": "sent"})));
    let (status, refused) = fresh("POST", "/api/system/pid-test", Some("nonsense")).await;
    assert_eq!((status, refused), (400, json!({"error": "kind must be velocity or position"})));
}

#[tokio::test]
async fn a_pid_test_is_refused_while_a_job_runs() {
    let mut machine = Machine::new("h", Settings::defaults());
    let now = std::time::Duration::ZERO;
    machine.handle_inline(Event::PortOpened { port: "p".into() }, now);
    machine.handle_inline(Event::ControllerLine("ok".into()), now);
    machine.load_gcode("G0 X1\nG0 X2\n");
    machine.handle_inline(Event::Action(wc_protocol::Action::new("startRun", json!(""), json!(""))), now);
    let body = r#"{"kind": "velocity", "motor": "L", "start": "0", "stop": "1", "steps": "1", "version": "1",
        "kp": "1", "ki": "0", "kd": "0"}"#;
    let (status, _) = call(machine, Arc::new(NoHost), "POST", "/api/system/pid-test", Some(body)).await;
    assert_eq!(status, 409);
}

/// The save reaches the shared file in Python's format, and Python's own reading of it is what Rust reads back.
#[tokio::test]
async fn a_saved_setting_is_written_to_webcontrol_json() {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths::new(home.path());
    let settings = Settings::open(&paths).unwrap();
    let host = Arc::new(SystemHost::new(&paths.dir(), Arc::new(Notify::new())).saving_to(paths.clone()));
    let body = r#"{"values": {"motorSpacingX": "3000", "zAxis": true}}"#;
    let (status, _) = call(Machine::new("h", settings), host, "PUT", "/api/settings/maslow", Some(body)).await;
    assert_eq!(status, 200);
    let reread = Settings::load(&paths);
    assert_eq!(reread.value("Maslow Settings", "motorSpacingX"), Some(&json!("3000")));
    assert_eq!(reread.value("Maslow Settings", "zAxis"), Some(&json!(1)));
    assert_eq!(std::fs::read_to_string(paths.settings_file()).unwrap(), reread.to_python_json());
    assert!(paths.settings_backup().exists());
}

#[test]
fn a_host_without_a_home_keeps_settings_in_memory_and_reports_a_failed_save() {
    let dir = tempfile::tempdir().unwrap();
    SystemHost::new(dir.path(), Arc::new(Notify::new())).save_settings(&Settings::defaults());
    assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
    let blocked = Paths::new(&dir.path().join("file"));
    std::fs::write(dir.path().join("file"), "").unwrap();
    let host = SystemHost::new(dir.path(), Arc::new(Notify::new())).saving_to(blocked.clone());
    host.save_settings(&Settings::defaults());
    assert!(!blocked.settings_file().exists());
}

#[tokio::test]
async fn an_answer_after_the_machine_stopped_is_a_503() {
    let (machine, task) = spawn(Machine::new("h", Settings::defaults()), Box::new(NoPort), Arc::new(NoHost));
    task.abort();
    let _ = task.await;
    let app =
        router(AppState::new(machine, PingConfig::default(), Paths::new(std::path::Path::new("/nonexistent-home"))));
    let request =
        Request::builder().method("POST").uri("/api/system/pid-test").header(header::CONTENT_TYPE, "application/json");
    let request = request.body(Body::from("{}")).unwrap();
    let response = tower::ServiceExt::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}
