//! Optical calibration's routes: refused while the setting is off, stubs once it is on, and the setting reaching
//! `GET /api/state` without a restart.

mod support;

use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use serde_json::{Value, json};
use support::Server;
use wc_config::{Paths, Settings};
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

#[tokio::test]
async fn every_optical_route_is_refused_while_the_setting_is_off() {
    let server = Server::new();
    for (method, path, body) in routes() {
        let reply = match body {
            Some(body) => server.send_json(method, path, body).await,
            None => server.get(path).await,
        };
        assert_eq!(reply.status, StatusCode::CONFLICT, "{method} {path}");
        assert!(reply.json()["error"].as_str().unwrap().contains("Settings > WebControl Settings"), "{path}");
    }
}

#[tokio::test]
async fn turning_the_setting_on_reaches_the_state_and_the_stubs_at_once() {
    let server = Server::new();
    assert_eq!(server.get("/api/state").await.json()["experimental"], json!([]));
    turn_optical(&server, true).await;
    assert_eq!(server.get("/api/state").await.json()["experimental"], json!(["optical"]));
    for (method, path, body) in routes() {
        let body = body.map(|b| b.to_string().into_bytes()).unwrap_or_default();
        let reply = server.call(method, path, Some("application/json"), body).await;
        assert_eq!(reply.status, StatusCode::NOT_IMPLEMENTED, "{method} {path}");
        assert_eq!(reply.json()["error"], "This part of optical calibration is not built yet.");
    }
    turn_optical(&server, false).await;
    assert_eq!(server.get("/api/calibration/optical").await.status, StatusCode::CONFLICT);
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
