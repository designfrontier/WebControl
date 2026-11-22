//! The tool library routes over HTTP, every body checked against the contract.

mod support;

use std::time::Duration;

use axum::http::StatusCode;
use serde_json::{Value, json};
use support::{Server, machine_with};
use wc_config::Settings;
use wc_machine::Event;
use wc_protocol::{Action, Push};

fn library_pushes(pushes: &mut tokio::sync::broadcast::Receiver<Push>) -> Vec<Value> {
    std::iter::from_fn(|| pushes.try_recv().ok())
        .filter(|push| push.data["command"] == "toolLibrary")
        .map(|push| serde_json::from_str(push.data["data"].as_str().unwrap()).unwrap())
        .collect()
}

fn endmill(number: u32) -> Value {
    json!({"number": number, "description": "1/4\" 2 flute endmill", "diameter": 6.35, "units": "mm", "flutes": 2})
}

fn stored(number: u32, description: &str) -> Value {
    json!({"number": number, "description": description, "diameter": null, "units": null, "flutes": null, "notes": ""})
}

#[tokio::test]
async fn tools_are_added_listed_edited_and_removed_and_every_client_hears() {
    let server = Server::new();
    let mut pushes = server.machine.subscribe();
    assert_eq!(server.get("/api/tools").await.json(), json!({"tools": []}));

    let added = server.post_json("/api/tools", endmill(3)).await;
    assert_eq!(added.status, StatusCode::CREATED);
    let three = json!({"number": 3, "description": "1/4\" 2 flute endmill", "diameter": 6.35, "units": "mm",
        "flutes": 2, "notes": ""});
    assert_eq!(added.json(), json!({"tools": [three]}));
    server.post_json("/api/tools", json!({"number": 1, "description": "v-bit"})).await;
    assert_eq!(server.get("/api/tools").await.json(), json!({"tools": [stored(1, "v-bit"), three]}));

    let renumbered = server.send_json("PUT", "/api/tools/1", json!({"number": 2, "description": "60° v-bit"})).await;
    assert_eq!(
        (renumbered.status, renumbered.json()),
        (StatusCode::OK, json!({"tools": [stored(2, "60° v-bit"), three]}))
    );
    let removed = server.call("DELETE", "/api/tools/3", None, Vec::new()).await;
    assert_eq!((removed.status, removed.json()), (StatusCode::OK, json!({"tools": [stored(2, "60° v-bit")]})));

    let heard = library_pushes(&mut pushes);
    assert_eq!(heard.len(), 4);
    assert_eq!(heard[3], json!({"tools": [stored(2, "60° v-bit")]}));
    let file: Value = serde_json::from_str(&std::fs::read_to_string(server.dir().join("tools.json")).unwrap()).unwrap();
    assert_eq!(file, json!({"tools": [stored(2, "60° v-bit")]}));
}

#[tokio::test]
async fn bad_tools_taken_numbers_and_missing_tools_are_refused_unchanged() {
    let server = Server::new();
    let mut pushes = server.machine.subscribe();
    server.post_json("/api/tools", endmill(1)).await;
    server.post_json("/api/tools", endmill(2)).await;
    library_pushes(&mut pushes);

    let bad = server.call("POST", "/api/tools", Some("application/json"), b"{\"number\": 4}".to_vec()).await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    assert!(bad.json()["error"].as_str().unwrap().starts_with("Describe the tool"));
    let garbled = server.call("PUT", "/api/tools/1", Some("application/json"), b"nope".to_vec()).await;
    assert_eq!(garbled.status, StatusCode::BAD_REQUEST);
    let taken = server.post_json("/api/tools", endmill(1)).await;
    assert_eq!(
        (taken.status, taken.json()["error"].clone()),
        (StatusCode::CONFLICT, json!("Your tool library already has a tool 1."))
    );
    assert_eq!(server.send_json("PUT", "/api/tools/1", endmill(2)).await.status, StatusCode::CONFLICT);
    assert_eq!(server.send_json("PUT", "/api/tools/9", endmill(9)).await.status, StatusCode::NOT_FOUND);
    let missing = server.call("DELETE", "/api/tools/9", None, Vec::new()).await;
    assert_eq!(
        (missing.status, missing.json()["error"].clone()),
        (StatusCode::NOT_FOUND, json!("Your tool library has no tool 9."))
    );
    let word = server.call("DELETE", "/api/tools/one", None, Vec::new()).await;
    assert_eq!(word.status, StatusCode::NOT_FOUND);
    let word =
        server.call("PUT", "/api/tools/one", Some("application/json"), endmill(1).to_string().into_bytes()).await;
    assert_eq!(word.status, StatusCode::NOT_FOUND);

    assert!(library_pushes(&mut pushes).is_empty());
    assert_eq!(server.get("/api/tools").await.json()["tools"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn the_library_can_be_edited_during_a_job() {
    let mut machine = machine_with(Settings::defaults());
    machine.handle_inline(Event::PortOpened { port: "/dev/ttyACM0".into() }, Duration::ZERO);
    machine.load_gcode("G0 X1\nG1 Z-1\n");
    machine.handle_inline(Event::Action(Action::new("startRun", json!(""), json!(""))), Duration::ZERO);
    let server = Server::with(machine);
    assert!(server.machine.snapshot().gcode_locked);
    assert_eq!(server.post_json("/api/tools", endmill(1)).await.status, StatusCode::CREATED);
}

#[tokio::test]
async fn a_library_that_cannot_be_read_or_written_is_a_server_error() {
    let server = Server::new();
    std::fs::create_dir_all(server.dir().join("tools.json")).unwrap();
    let read = server.get("/api/tools").await;
    assert_eq!(read.status, StatusCode::INTERNAL_SERVER_ERROR);
    let write = server.post_json("/api/tools", endmill(1)).await;
    assert_eq!(write.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(write.json()["error"].as_str().unwrap().starts_with("The tool library could not be saved"));
}
