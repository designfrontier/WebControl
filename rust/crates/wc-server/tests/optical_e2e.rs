//! Optical calibration end to end on `--simulate`: the simulated controller, built to the default geometry but
//! configured 3 mm narrower and 5 mm lower, the synthetic camera rendering the target where the sled really is, and
//! the real detector, scan, fit and Accept behind the HTTP routes.

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use serde_json::{Value, json};
use support::Server;
use wc_config::{Paths, Settings};
use wc_machine::{MachineHandle, optical, spawn};
use wc_protocol::main::LinkStatus;
use wc_server::camera::Cameras;
use wc_server::host::{LOG_FILE, SystemHost};
use wc_server::serial::SimOpener;
use wc_server::{AppState, PingConfig};

const TRUE_SPACING: f64 = 2978.4;
const TRUE_OFFSET_Y: f64 = 463.0;
const SPACING_ERROR: f64 = 3.0;
const OFFSET_ERROR: f64 = -5.0;

struct Simulated {
    server: Server,
    sim: Arc<std::sync::Mutex<wc_sim::Sim>>,
    cameras: Cameras,
    _streaming: wc_server::camera::Streaming,
}

fn simulated(changes: &[(&str, &str, Value)]) -> Simulated {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths::new(home.path());
    let mut settings = Settings::defaults();
    [
        ("WebControl Settings", "opticalCalibrationEnabled", json!(1)),
        ("Camera Settings", "cameraSource", json!("synthetic")),
        ("Camera Settings", "fps", json!(30)),
        ("Maslow Settings", "zAxis", json!(1)),
        ("Maslow Settings", "motorSpacingX", json!((TRUE_SPACING - SPACING_ERROR).to_string())),
        ("Maslow Settings", "motorOffsetY", json!((TRUE_OFFSET_Y - OFFSET_ERROR).to_string())),
    ]
    .iter()
    .chain(changes)
    .for_each(|(section, key, value)| assert!(settings.set_value(section, key, value.clone()), "{key}"));
    let opener = SimOpener::new(wc_sim::Build::holey_51_27());
    let sim = opener.sim();
    let machine = support::machine_with(settings).on_port("simulator");
    let host = Arc::new(SystemHost::new(&paths.dir(), Arc::new(tokio::sync::Notify::new())));
    let (machine, _task) = spawn(machine, Box::new(opener), host.clone());
    let cameras = Cameras::new(Some(wc_server::cli::true_sled_position(sim.clone(), machine.clone())));
    let streaming = cameras.stream_to(machine.clone());
    let state = AppState::new(machine.clone(), PingConfig::default(), paths).with_cameras(cameras.clone());
    let server = Server { app: wc_server::router(state), home, machine, host };
    Simulated { server, sim, cameras, _streaming: streaming }
}

async fn until(what: &str, limit: Duration, done: impl AsyncFn() -> bool) {
    let waited = async {
        while !done().await {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    tokio::time::timeout(limit, waited).await.unwrap_or_else(|_| panic!("timed out waiting for {what}"));
}

async fn connected(machine: &MachineHandle) {
    until("the controller", Duration::from_secs(10), async || machine.snapshot().link == LinkStatus::Connected).await;
    // The boot handshake and its settings sync go first.
    tokio::time::sleep(Duration::from_millis(1500)).await;
}

/// The 5 x 5 squares around the centre: the fewest that pin the geometry down to 0.5 mm.
fn scan_settings(tl: (i32, i32), br: (i32, i32)) -> Value {
    json!({"markerX": 0.6, "markerY": 0.6, "scaleX": 1.0, "scaleY": 1.0, "opticalCenterX": 0.0,
        "opticalCenterY": 0.0, "positionTolerance": 0.125, "tlX": tl.0, "tlY": tl.1, "brX": br.0, "brY": br.1,
        "calibrationExtents": "Custom", "autoScanDirection": 0, "gaussianBlurValue": 5, "cannyLowValue": 50.0,
        "cannyHighValue": 100.0, "feedRate": 3000.0, "settleTime": 0.1})
}

async fn session(server: &Server) -> Value {
    server.get("/api/calibration/optical").await.json()["session"].clone()
}

async fn finished(server: &Server, limit: Duration) -> Value {
    until("the scan", limit, async || {
        matches!(session(server).await["state"].as_str(), Some("done" | "failed" | "stopped"))
    })
    .await;
    session(server).await
}

/// Every message the machine broadcasts from now on.
fn watch(machine: &MachineHandle) -> Arc<std::sync::Mutex<Vec<Value>>> {
    let (seen, mut pushes) = (Arc::new(std::sync::Mutex::new(Vec::new())), machine.subscribe());
    let keep = seen.clone();
    tokio::spawn(async move {
        while let Ok(push) = pushes.recv().await {
            keep.lock().unwrap().push(push.data);
        }
    });
    seen
}

fn notes(seen: &std::sync::Mutex<Vec<Value>>) -> Vec<String> {
    let seen = seen.lock().unwrap();
    let wanted = |m: &&Value| m.to_string().contains("opticalScanProgress") || m.to_string().contains("activateModal");
    seen.iter().filter(wanted).map(|m| m.to_string().chars().take(300).collect()).collect()
}

fn measured(session: &Value) -> usize {
    let columns = session["errorX"].as_array().cloned().unwrap_or_default();
    columns.iter().flat_map(|column| column.as_array().cloned().unwrap_or_default()).filter(|v| !v.is_null()).count()
}

fn sent(server: &Server) -> Vec<String> {
    server.host.flush_log();
    let log = std::fs::read_to_string(server.dir().join(LOG_FILE)).unwrap_or_default();
    log.lines().filter_map(|line| line.split_once(": Sent: ").map(|(_, sent)| sent.to_string())).collect()
}

fn number(value: &Value) -> f64 {
    value.as_f64().unwrap_or_else(|| value.as_str().and_then(|s| s.parse().ok()).expect("a number"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_scan_measures_the_misconfigured_geometry_and_accept_writes_the_four_settings() {
    let Simulated { server, .. } = simulated(&[]);
    connected(&server.machine).await;
    let seen = watch(&server.machine);
    let started = Instant::now();
    let scan = server.post_json("/api/calibration/optical/scan", json!({"settings": scan_settings((-2, 2), (2, -2))}));
    assert_eq!(scan.await.json(), json!({"total": 25}));
    let done = finished(&server, Duration::from_secs(240)).await;
    assert_eq!(done["state"], "done", "{:#?}", notes(&seen));
    println!("25 squares in {:?}", started.elapsed());
    let orientation = &done["orientation"];
    assert!(orientation["rotationDeg"].as_f64().unwrap().abs() < 0.5, "{orientation}");
    assert_eq!(orientation["mirrored"], false);
    assert!((orientation["pxPerMm"].as_f64().unwrap() - 6.0).abs() < 0.1, "{orientation}");

    let fit = server.post_json("/api/calibration/optical/fit", json!({})).await;
    assert_eq!(fit.status, StatusCode::OK, "{}", fit.json());
    let fit = fit.json();
    let fitted = &fit["fitted"];
    let misfit = [
        number(&fitted["distanceBetweenMotors"]) - TRUE_SPACING,
        number(&fitted["motorYoffset"]) - TRUE_OFFSET_Y,
        number(&fitted["leftChainTolerance"]) * 18.0,
        number(&fitted["rightChainTolerance"]) * 18.0,
    ];
    println!("fit {fit}\nmisfit (mm) {misfit:?}");
    assert!(misfit.iter().all(|m| m.abs() < 0.5), "{misfit:?}");
    assert!(fit["rmsMm"].as_f64().unwrap() < 0.1, "{fit}");

    let before = sent(&server).len();
    let accepted = server.post_json("/api/calibration/optical/accept", json!({})).await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.json());
    let _ = server.machine.request(wc_machine::files::Request::SetSettings(Vec::new())).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let writes: Vec<String> = sent(&server)[before..].iter().filter(|line| line.starts_with('$')).cloned().collect();
    let keys: Vec<&str> = writes.iter().map(|line| line.split('=').next().unwrap()).collect();
    assert_eq!(keys, ["$3", "$2", "$40", "$41"], "holey calibration's Accept order: {writes:?}");
    let saved = server.machine.snapshot().settings.clone();
    let value = |key: &str| saved.value("Maslow Settings", key).map(|v| number(&v)).unwrap();
    assert!((value("motorSpacingX") - TRUE_SPACING).abs() < 0.5);
    assert!((value("motorOffsetY") - TRUE_OFFSET_Y).abs() < 0.5);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stop_mid_scan_keeps_the_squares_measured_and_restores_the_modes() {
    let Simulated { server, sim, .. } = simulated(&[]);
    connected(&server.machine).await;
    let scan = json!({"settings": scan_settings((-1, 1), (1, 0))});
    assert_eq!(server.post_json("/api/calibration/optical/scan", scan).await.json(), json!({"total": 6}));
    until("a measured square", Duration::from_secs(60), async || measured(&session(&server).await) > 0).await;
    server.act("stopRun", json!(""));
    let stopped = finished(&server, Duration::from_secs(10)).await;
    assert_eq!(stopped["state"], "stopped");
    let position = sim.lock().unwrap().position();
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(sim.lock().unwrap().position(), position, "no queued motion survives Stop");
    let after = sent(&server);
    let tail: Vec<&str> = after.iter().rev().take(3).map(|line| line.trim()).collect();
    assert!(tail.contains(&"G90") && tail.contains(&"!"), "{tail:?}");
    let csv = server.get("/api/calibration/optical/csv").await;
    assert_eq!(csv.status, StatusCode::OK, "the partial grid is kept for review");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_alarm_mid_scan_ends_it() {
    let Simulated { server, sim, .. } = simulated(&[]);
    connected(&server.machine).await;
    let scan = json!({"settings": scan_settings((-1, 1), (1, 0))});
    assert_eq!(server.post_json("/api/calibration/optical/scan", scan).await.status, StatusCode::OK);
    until("the scan to move", Duration::from_secs(30), async || {
        matches!(session(&server).await["state"].as_str(), Some("measuring" | "moving" | "settling"))
    })
    .await;
    let alarm = sim.lock().unwrap().alarm("Position Lost");
    alarm.into_iter().for_each(|line| server.machine.send(wc_machine::Event::ControllerLine(line)));
    let ended = finished(&server, Duration::from_secs(10)).await;
    assert_ne!(ended["state"], "done", "{ended}");
    assert!(server.machine.snapshot().optical.scan.is_none());
}

#[tokio::test]
async fn with_the_setting_off_no_camera_opens_and_every_route_is_refused() {
    let Simulated { server, cameras, .. } =
        simulated(&[("WebControl Settings", "opticalCalibrationEnabled", json!(0))]);
    let state = server.get("/api/state").await.json();
    assert_eq!(state["experimental"], json!([]));
    let refused = server.post_json("/api/calibration/optical/scan", json!({"settings": scan_settings((0, 0), (0, 0))}));
    let refused = refused.await;
    assert_eq!((refused.status, refused.json()["error"].as_str().unwrap()), (StatusCode::CONFLICT, optical::OFF));
    server.act("toggleCamera", json!(""));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!server.machine.snapshot().camera, "the overlay stays off");
    assert!(!cameras.running(), "no camera opened");
    let image =
        server.post_json("/api/calibration/optical/test-image", json!({"settings": scan_settings((0, 0), (0, 0))}));
    assert_eq!(image.await.status, StatusCode::CONFLICT);
    assert!(!cameras.running(), "no camera opened");
}
