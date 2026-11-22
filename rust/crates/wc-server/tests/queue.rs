//! `/api/queue`: the routes against the contract, the saved list, and two files run back to back on the simulator.

mod support;

use std::sync::Arc;
use std::time::Duration;

use axum::http::StatusCode;
use serde_json::{Value, json};
use support::Server;
use wc_config::{Paths, Settings};
use wc_machine::{Event, spawn};
use wc_protocol::Action;
use wc_protocol::main::LinkStatus;
use wc_server::host::SystemHost;
use wc_server::serial::SimOpener;
use wc_server::{AppState, PingConfig};

fn files(queue: &Value) -> Vec<&str> {
    queue["items"].as_array().unwrap().iter().map(|item| item["file"].as_str().unwrap()).collect()
}

async fn until(what: &str, done: impl Fn() -> bool) {
    let waited = async {
        while !done() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(20), waited).await.unwrap_or_else(|_| panic!("timed out: {what}"));
}

#[tokio::test]
async fn files_are_added_moved_removed_cleared_and_saved() {
    let server = Server::new();
    server.write("gcode/a.nc", "T1 M6\nG1 X1\n");
    server.write("gcode/jobs/b.nc", "(T9)\nG1 X2\n");
    let added = server.post_json("/api/queue/add", json!({"file": "./a.nc"})).await;
    assert_eq!(added.status, StatusCode::OK);
    let queue = server.post_json("/api/queue/add", json!({"file": "jobs/b.nc"})).await.json();
    assert_eq!(files(&queue), ["a.nc", "jobs/b.nc"]);
    assert_eq!((queue["items"][0]["tool"].clone(), queue["items"][1]["between"].clone()), (json!(1), json!("noTool")));
    let missing = server.post_json("/api/queue/add", json!({"file": "nope.nc"})).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    let outside = server.post_json("/api/queue/add", json!({"file": "../webcontrol.json"})).await;
    assert_eq!(outside.status, StatusCode::NOT_FOUND);

    let second = queue["items"][1]["id"].clone();
    let moved = server.post_json("/api/queue/move", json!({"id": second, "to": 0})).await.json();
    assert_eq!(files(&moved), ["jobs/b.nc", "a.nc"]);
    assert_eq!(server.post_json("/api/queue/remove", json!({"id": 999})).await.status, StatusCode::NOT_FOUND);
    let removed = server.post_json("/api/queue/remove", json!({"id": second})).await.json();
    assert_eq!(files(&removed), ["a.nc"]);
    let first = removed["items"][0]["id"].clone();
    let overridden = server.post_json("/api/queue/override", json!({"id": first, "skipBitChanges": true})).await;
    assert_eq!(overridden.json()["items"][0]["skipBitChanges"], true);
    let gone = server.post_json("/api/queue/override", json!({"id": 999, "skipBitChanges": true})).await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
    let skipping = server.post_json("/api/queue/add", json!({"file": "a.nc", "skipBitChanges": true})).await.json();
    assert_eq!(skipping["items"][1]["between"], "sameTool", "the first file names tool 1 too");
    assert_eq!(files(&server.get("/api/queue").await.json()), ["a.nc", "a.nc"]);
    server.settle().await;
    let saved = server.dir().join("queue.json");
    let wanted = r#"{"file":"a.nc","skipBitChanges":true}"#;
    until("the list saved", || {
        std::fs::read_to_string(&saved).is_ok_and(|text| text.replace([' ', '\n'], "").contains(wanted))
    })
    .await;

    let cleared = server.post_json("/api/queue/clear", json!({})).await.json();
    assert_eq!((cleared["items"].clone(), cleared["state"].clone()), (json!([]), json!("idle")));
}

#[tokio::test]
async fn malformed_bodies_and_a_continue_without_a_controller_are_refused() {
    let server = Server::new();
    let bad = server.call("POST", "/api/queue/move", Some("application/json"), b"{\"id\": \"x\"}".to_vec()).await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    let extra = server.call("POST", "/api/queue/clear", Some("application/json"), b"{\"all\": 1}".to_vec()).await;
    assert_eq!(extra.status, StatusCode::BAD_REQUEST);
    let wrong =
        server.call("POST", "/api/queue/continue", Some("application/json"), b"{\"from\": \"x\"}".to_vec()).await;
    assert_eq!(wrong.status, StatusCode::BAD_REQUEST);
    let empty = server.post_json("/api/queue/continue", json!({})).await;
    assert_eq!((empty.status, empty.json()["error"].clone()), (StatusCode::CONFLICT, json!("The queue is empty.")));
    server.write("gcode/a.nc", "G1 X1\n");
    server.post_json("/api/queue/add", json!({"file": "a.nc"})).await;
    let offline = server.post_json("/api/queue/continue", json!({"from": "stopped"})).await;
    assert_eq!(offline.json()["error"], "The controller is not connected.");
}

#[tokio::test]
async fn a_saved_queue_is_restored_with_each_files_tools() {
    let home = tempfile::tempdir().unwrap();
    let dir = home.path().join(".WebControl");
    std::fs::create_dir_all(dir.join("gcode")).unwrap();
    std::fs::write(dir.join("gcode/a.nc"), "T3 M6\nG1 X1\n").unwrap();
    let saved = r#"[{"file": "a.nc", "skipBitChanges": true}, {"file": "gone.nc"}]"#;
    std::fs::write(dir.join("queue.json"), saved).unwrap();
    let items = wc_server::queue::restore(&dir).await;
    assert_eq!(
        items.iter().map(|item| (item.file.as_str(), item.tools.first, item.skip_bit_changes)).collect::<Vec<_>>(),
        [("a.nc", Some(3), true), ("gone.nc", None, false)]
    );
    assert!(wc_server::queue::restore(home.path()).await.is_empty());
}

#[tokio::test]
async fn two_files_run_back_to_back_on_the_simulator() {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths::new(home.path());
    std::fs::create_dir_all(paths.gcode_dir()).unwrap();
    std::fs::write(paths.gcode_dir().join("a.nc"), "T1 M6\nG21\nG90\nG0 Z5\nG1 X2 F600\nG0 Z5\n").unwrap();
    std::fs::write(paths.gcode_dir().join("b.nc"), "T1 M6\nG21\nG90\nG1 X4 F600\nG0 Z5\nM30\n").unwrap();
    let mut settings = Settings::defaults();
    settings.set_value("Maslow Settings", "zAxis", json!(1));
    let machine = support::machine_with(settings).on_port("simulator").with_library(paths.gcode_dir());
    let host = Arc::new(SystemHost::new(&paths.dir(), Arc::new(tokio::sync::Notify::new())));
    let (machine, _task) = spawn(machine, Box::new(SimOpener::new(wc_sim::Build::holey_51_27())), host.clone());
    let state = AppState::new(machine.clone(), PingConfig::default(), paths);
    let _service = wc_server::queue::serve(&state);
    let server = Server { app: wc_server::router(state), home, machine: machine.clone(), host };
    until("the controller", || machine.snapshot().link == LinkStatus::Connected).await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    server.post_json("/api/queue/add", json!({"file": "a.nc"})).await;
    server.post_json("/api/queue/add", json!({"file": "b.nc"})).await;
    let started = server.post_json("/api/queue/continue", json!({})).await;
    assert_eq!(started.status, StatusCode::OK, "{:?}", started.json());
    until("the tool change prompt", || machine.snapshot().manual_z_axis_adjust).await;
    machine.send(Event::Action(Action::new("resumeRun", json!(""), json!(""))));
    until("both files", || machine.snapshot().queue.items.is_empty()).await;
    until("the last line", || !machine.snapshot().job_in_progress).await;
    let snapshot = machine.snapshot();
    assert_eq!((snapshot.gcode_name.as_deref(), snapshot.current_tool), (Some("b.nc"), 1));
    assert_eq!(serde_json::to_value(&*snapshot.queue).unwrap()["state"], "idle");
}
