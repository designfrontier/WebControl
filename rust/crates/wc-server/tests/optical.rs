//! Optical calibration's routes: refused while the setting is off, then each route's answers, every JSON body
//! checked against the contract; the setting reaches `GET /api/state` without a restart.

mod support;

use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use serde_json::{Value, json};
use support::Server;
use wc_config::{Paths, Settings};
use wc_machine::optical;
use wc_machine::{NoHost, NoPort, spawn};
use wc_server::camera::Cameras;
use wc_server::{AppState, PingConfig};

fn settings() -> Value {
    json!({"markerX": 0.6, "markerY": 0.6, "scaleX": 1.0, "scaleY": 1.0, "opticalCenterX": 0.0,
        "opticalCenterY": 0.0, "positionTolerance": 0.125, "tlX": -15, "tlY": 7, "brX": 15, "brY": -7,
        "calibrationExtents": "Full Area", "autoScanDirection": 0, "gaussianBlurValue": 5, "cannyLowValue": 50.0,
        "cannyHighValue": 100.0, "feedRate": 500.0, "settleTime": 1.0})
}

fn routes() -> Vec<(&'static str, &'static str, Option<Value>)> {
    vec![
        ("GET", "/api/calibration/optical", None),
        ("PUT", "/api/calibration/optical/settings", Some(settings())),
        ("POST", "/api/calibration/optical/scan", Some(json!({"settings": settings()}))),
        ("POST", "/api/calibration/optical/test-image", Some(json!({"settings": settings()}))),
        ("POST", "/api/calibration/optical/find-center", Some(json!({"settings": settings()}))),
        ("POST", "/api/calibration/optical/fit", Some(json!({}))),
        ("POST", "/api/calibration/optical/accept", Some(json!({}))),
        ("GET", "/api/calibration/optical/csv", None),
        ("GET", "/api/calibration/optical/target.pdf?paper=a4", None),
    ]
}

async fn turn_optical(server: &Server, on: bool) {
    let body = json!({"values": {"opticalCalibrationEnabled": on}});
    assert_eq!(server.send_json("PUT", "/api/settings/webcontrol", body).await.status, StatusCode::OK);
}

async fn on() -> Server {
    let server = Server::new();
    turn_optical(&server, true).await;
    server
}

async fn camera_source(server: &Server, source: &str) {
    let body = json!({"values": {"cameraSource": source}});
    assert_eq!(server.send_json("PUT", "/api/settings/camera", body).await.status, StatusCode::OK);
}

fn error(reply: &support::Reply) -> String {
    reply.json()["error"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn every_optical_route_is_refused_while_the_setting_is_off() {
    let server = Server::new();
    for (method, path, body) in routes() {
        let reply = match body {
            Some(body) => server.send_json(method, path, body).await,
            None => server.get(path).await,
        };
        assert_eq!(reply.status, StatusCode::CONFLICT, "{method} {path}");
        assert!(error(&reply).contains("Settings > WebControl Settings"), "{path}");
    }
}

#[tokio::test]
async fn turning_the_setting_on_reaches_the_state_and_the_routes_at_once() {
    let server = Server::new();
    assert_eq!(server.get("/api/state").await.json()["experimental"], json!([]));
    turn_optical(&server, true).await;
    assert_eq!(server.get("/api/state").await.json()["experimental"], json!(["optical"]));
    let state = server.get("/api/calibration/optical").await;
    assert_eq!(state.status, StatusCode::OK);
    assert_eq!(state.json()["settings"], settings());
    assert_eq!(state.json()["session"], Value::Null);
    assert_eq!(state.json()["camera"], json!({"source": "auto", "available": true}));
    turn_optical(&server, false).await;
    assert_eq!(server.get("/api/calibration/optical").await.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn the_camera_status_names_the_source_and_whether_it_can_open() {
    let server = on().await;
    camera_source(&server, "synthetic").await;
    let camera = server.get("/api/calibration/optical").await.json()["camera"].clone();
    assert_eq!(camera, json!({"source": "synthetic", "available": false}), "synthetic needs --simulate");
    camera_source(&server, "webcam").await;
    assert_eq!(server.get("/api/calibration/optical").await.json()["camera"]["available"], false);
}

#[tokio::test]
async fn settings_are_saved_and_a_bad_body_is_refused() {
    let server = on().await;
    let mut changed = settings();
    changed["feedRate"] = json!(800.0);
    changed["tlX"] = json!(-2);
    let saved = server.send_json("PUT", "/api/calibration/optical/settings", changed.clone()).await;
    assert_eq!((saved.status, saved.json()), (StatusCode::OK, json!({"settings": changed})));
    assert_eq!(server.get("/api/calibration/optical").await.json()["settings"]["feedRate"], 800.0);
    let mut zero = settings();
    zero["markerX"] = json!(0);
    let refused = server.send_json("PUT", "/api/calibration/optical/settings", zero).await;
    assert_eq!(
        (refused.status, error(&refused)),
        (StatusCode::BAD_REQUEST, "markerX must be a number above 0.".into())
    );
    let missing = server.call("PUT", "/api/calibration/optical/settings", None, b"{}".to_vec()).await;
    assert_eq!(missing.status, StatusCode::BAD_REQUEST);
    assert!(error(&missing).starts_with("Invalid request: missing field"));
}

#[tokio::test]
async fn a_scan_needs_a_connected_controller_and_valid_extents() {
    let server = on().await;
    let one = json!({"settings": settings(), "cell": {"i": 0, "j": 0}});
    let refused = server.post_json("/api/calibration/optical/scan", one).await;
    assert_eq!((refused.status, error(&refused)), (StatusCode::CONFLICT, optical::NOT_CONNECTED.into()));
    let extents = server
        .call("POST", "/api/calibration/optical/scan", None, json!({"settings": settings()}).to_string().into_bytes())
        .await;
    match wc_optical::grid::scan_order(&optical::settings(&Settings::defaults())) {
        Ok(_) => assert_eq!((extents.status, error(&extents)), (StatusCode::CONFLICT, optical::NOT_CONNECTED.into())),
        Err(problem) => assert_eq!((extents.status, error(&extents)), (StatusCode::BAD_REQUEST, problem.to_string())),
    }
    let garbled = server.call("POST", "/api/calibration/optical/scan", None, b"[1]".to_vec()).await;
    assert_eq!(garbled.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_image_and_find_center_report_a_camera_that_cannot_capture() {
    let server = on().await;
    for source in ["synthetic", "rpicam"] {
        camera_source(&server, source).await;
        for path in ["/api/calibration/optical/test-image", "/api/calibration/optical/find-center"] {
            let reply = server.call("POST", path, None, json!({"settings": settings()}).to_string().into_bytes()).await;
            assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{source} {path}");
            assert!(!error(&reply).is_empty());
        }
    }
    let garbled = server.call("POST", "/api/calibration/optical/test-image", None, b"{}".to_vec()).await;
    assert_eq!(garbled.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn fit_accept_and_csv_need_a_measured_grid() {
    let server = on().await;
    let fit = server.post_json("/api/calibration/optical/fit", json!({})).await;
    assert_eq!((fit.status, error(&fit)), (StatusCode::CONFLICT, optical::NOTHING_MEASURED.into()));
    let accept = server.post_json("/api/calibration/optical/accept", json!({})).await;
    assert_eq!((accept.status, error(&accept)), (StatusCode::CONFLICT, optical::NOTHING_TO_ACCEPT.into()));
    let csv = server.get("/api/calibration/optical/csv").await;
    assert_eq!((csv.status, error(&csv)), (StatusCode::CONFLICT, optical::NOTHING_MEASURED.into()));
}

#[tokio::test]
async fn the_printable_target_is_track_bs_pdf_at_the_settings_marker_size() {
    let server = on().await;
    let target = wc_optical::render::Target::from_settings(&optical::settings(&Settings::defaults()));
    for (query, paper, name) in [("a4", wc_optical::Paper::A4, "a4"), ("letter", wc_optical::Paper::Letter, "letter")] {
        let reply =
            server.call("GET", &format!("/api/calibration/optical/target.pdf?paper={query}"), None, Vec::new()).await;
        match wc_optical::target::pdf(&target, paper) {
            Ok(pdf) => {
                assert_eq!(
                    (reply.status, reply.content_type.as_str(), reply.body),
                    (StatusCode::OK, "application/pdf", pdf)
                );
                assert_eq!(reply.disposition.unwrap(), format!("attachment; filename=\"optical-target-{name}.pdf\""));
            }
            Err(problem) => {
                assert_eq!((reply.status, error(&reply)), (StatusCode::INTERNAL_SERVER_ERROR, problem.to_string()))
            }
        }
    }
}

#[tokio::test]
async fn the_synthetic_camera_sees_the_simulated_sled() {
    let sim = Arc::new(Mutex::new(wc_sim::Sim::new(wc_sim::Build::holey_51_27())));
    let [x, y, _] = sim.lock().unwrap().position();
    let (machine, _task) = spawn(support::machine_with(Settings::defaults()), Box::new(NoPort), Arc::new(NoHost));
    let state = AppState::new(machine, PingConfig::default(), Paths::new(std::path::Path::new("/nowhere")));
    assert!(state.cameras().sled().is_none());
    let state = state.with_cameras(Cameras::new(Some(wc_server::cli::sled_position(sim))));
    assert_eq!(state.cameras().sled().expect("a simulated sled")(), (x, y));
}

/// `--simulate`: the simulated controller and the synthetic camera, which looks at its sled.
fn simulated() -> Server {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths::new(home.path());
    let mut settings = Settings::defaults();
    [
        ("WebControl Settings", "opticalCalibrationEnabled", json!(1)),
        ("Camera Settings", "cameraSource", json!("synthetic")),
        ("Maslow Settings", "zAxis", json!(1)),
    ]
    .into_iter()
    .for_each(|(section, key, value)| assert!(settings.set_value(section, key, value)));
    let opener = wc_server::serial::SimOpener::new(wc_sim::Build::holey_51_27());
    let cameras = Cameras::new(Some(wc_server::cli::sled_position(opener.sim())));
    let machine = support::machine_with(settings).on_port("simulator");
    let host = Arc::new(wc_server::host::SystemHost::new(&paths.dir(), Arc::new(tokio::sync::Notify::new())));
    let (machine, _task) = spawn(machine, Box::new(opener), host.clone());
    let app = wc_server::router(AppState::new(machine.clone(), PingConfig::default(), paths).with_cameras(cameras));
    Server { app, home, machine, host }
}

async fn session_state(server: &Server) -> Value {
    server.get("/api/calibration/optical").await.json()["session"]["state"].clone()
}

#[tokio::test]
async fn a_scan_on_the_simulator_reaches_the_synthetic_camera_and_stop_ends_it() {
    let server = simulated();
    let connected = async {
        while server.machine.snapshot().link != wc_protocol::main::LinkStatus::Connected {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    };
    tokio::time::timeout(std::time::Duration::from_secs(10), connected).await.unwrap();
    let mut quick = settings();
    quick["settleTime"] = json!(0.0);
    let scan = json!({"settings": quick, "cell": {"i": 1, "j": 0}});
    let started = server.post_json("/api/calibration/optical/scan", scan).await;
    assert_eq!((started.status, started.json()), (StatusCode::OK, json!({"total": 1})));
    let answered = async {
        while !matches!(session_state(&server).await.as_str(), Some("failed" | "done")) {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    };
    tokio::time::timeout(std::time::Duration::from_secs(20), answered).await.expect("the camera answered");
    // Until tracks A and B land the synthetic camera cannot capture, so the square fails.
    if session_state(&server).await == "failed" {
        let fit = server.post_json("/api/calibration/optical/fit", json!({})).await;
        assert_eq!((fit.status, error(&fit)), (StatusCode::CONFLICT, optical::RUNNING.into()));
        let image = server.post_json("/api/calibration/optical/test-image", json!({"settings": settings()})).await;
        assert_eq!((image.status, error(&image)), (StatusCode::CONFLICT, optical::RUNNING.into()));
        server.act("stopRun", json!(""));
        let _ = server.machine.request(wc_machine::files::Request::SetSettings(Vec::new())).await;
        assert_eq!(session_state(&server).await, "stopped");
    }
}
