//! The gcode and board routes over HTTP against a throwaway home (ported from test_api_files, test_api_boards,
//! test_gcode_load_guard, test_safe_paths and test_api_viewer). Every response and request body is checked
//! against the contract.

mod support;

use std::path::Path;
use std::time::Duration;

use axum::http::StatusCode;
use serde_json::{Value, json};
use support::{Part, Server, machine_with};
use wc_config::Settings;
use wc_machine::{Event, Machine};
use wc_protocol::{Action, Push};

fn setting(settings: &Value, section: &str, key: &str) -> Value {
    settings[section].as_array().unwrap().iter().find(|o| o["key"] == key).unwrap()["value"].clone()
}

/// The pushes seen so far with this command, as (data, dataFormat).
fn drain(pushes: &mut tokio::sync::broadcast::Receiver<Push>, command: &str) -> Vec<Value> {
    std::iter::from_fn(|| pushes.try_recv().ok())
        .filter(|push| push.data["command"] == command)
        .map(|push| push.data["data"].clone())
        .collect()
}

const SQUARE: &str = "G21\nG90\nG0 X0 Y0 Z5\nG1 Z-1 F500\nG1 X50 Y0\nG1 X50 Y50\nG1 X0 Y50\nG1 X0 Y0\nG0 Z5\n";

#[tokio::test]
async fn the_library_lists_files_and_the_last_selection() {
    let server = Server::new();
    server.write("gcode/part.nc", "G0 X0\n");
    server.write("gcode/jobs/leg.nc", "G0 X0\n");
    server.write("gcode/.hidden.nc", "");
    let changes = vec![
        ("Computed Settings".into(), "lastSelectedDirectory".into(), json!("jobs/")),
        ("Maslow Settings".into(), "openFile".into(), json!("leg.nc")),
    ];
    server.machine.request(wc_machine::files::Request::SetSettings(changes)).await;
    let body = server.get("/api/files/gcode").await.json();
    assert_eq!(body["directories"], json!([".", "jobs"]));
    assert_eq!(body["files"], json!([{"directory": ".", "name": "part.nc"}, {"directory": "jobs", "name": "leg.nc"}]));
    assert_eq!((body["lastDirectory"].clone(), body["lastFile"].clone()), (json!("jobs"), json!("leg.nc")));
    assert!(body["validExtensions"].as_str().unwrap().contains(".nc"));
}

#[tokio::test]
async fn a_directory_is_created_sanitised_and_announced() {
    let server = Server::new();
    let mut pushes = server.machine.subscribe();
    let reply = server.post_json("/api/files/gcode/directories", json!({"name": " My Parts/../x "})).await;
    assert_eq!(reply.status, StatusCode::CREATED);
    assert_eq!(reply.json()["directory"], "My_Parts_.._x");
    assert!(reply.json()["directories"].as_array().unwrap().contains(&json!("My_Parts_.._x")));
    assert!(server.dir().join("gcode/My_Parts_.._x").is_dir());
    server.settle().await;
    assert_eq!(drain(&mut pushes, "updateDirectories"), [json!("{\"directory\":\"My_Parts_.._x\"}")]);
    assert_eq!(server.post_json("/api/files/gcode/directories", json!({"name": "My_Parts_.._x"})).await.status, 201);
}

#[tokio::test]
async fn names_that_sanitise_to_nothing_or_lead_out_are_refused() {
    let server = Server::new();
    let mut pushes = server.machine.subscribe();
    for body in [json!({"name": "  "}), json!({"name": "../"})] {
        let reply = server.post_json("/api/files/gcode/directories", body).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST);
        assert!(reply.json()["error"].as_str().unwrap().contains("directory name"));
    }
    let reply = server.post_raw("/api/files/gcode/directories", "application/json", b"nope").await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    let reply = server.post_raw("/api/files/gcode/directories", "text/plain", b"{\"name\": \"x\"}").await;
    assert_eq!(reply.status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), server.dir().join("gcode/out")).unwrap();
        let reply = server.post_json("/api/files/gcode/directories", json!({"name": "out"})).await;
        assert_eq!(
            (reply.status, reply.json()["error"].clone()),
            (StatusCode::BAD_REQUEST, json!("That directory points outside the library."))
        );
    }
    server.settle().await;
    assert!(drain(&mut pushes, "updateDirectories").is_empty());
}

#[tokio::test]
async fn a_directory_that_cannot_be_made_is_a_server_error() {
    let server = Server::new();
    server.write("boards/taken", "");
    let reply = server
        .call("POST", "/api/boards/directories", Some("application/json"), br#"{"name": "taken"}"#.to_vec())
        .await;
    assert_eq!(reply.status, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn a_file_or_a_directory_with_everything_in_it_is_deleted() {
    let server = Server::new();
    server.write("gcode/part.nc", "G0 X0\n");
    server.write("gcode/keep.nc", "G0 X0\n");
    server.write("gcode/jobs/leg.nc", "G0 X0\n");
    server.write("gcode/jobs/deeper/arm.nc", "G0 X0\n");
    let reply = server.post_json("/api/files/gcode/delete", json!({"path": "./part.nc"})).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(
        reply.json()["files"],
        json!([{"directory": ".", "name": "keep.nc"}, {"directory": "jobs", "name": "leg.nc"}])
    );
    assert!(!server.dir().join("gcode/part.nc").exists());
    let reply = server.post_json("/api/files/gcode/delete", json!({"path": "jobs"})).await;
    assert_eq!(reply.json(), json!({"directories": ["."], "files": [{"directory": ".", "name": "keep.nc"}]}));
    assert!(!server.dir().join("gcode/jobs").exists());
}

#[tokio::test]
async fn the_loaded_file_a_queued_file_and_their_directories_are_kept() {
    let server = Server::new();
    server.write("gcode/jobs/leg.nc", SQUARE);
    server.write("gcode/queued/a.nc", SQUARE);
    server.write("gcode/jobs2/other.nc", SQUARE);
    assert_eq!(server.post_form("/openGCode", &[Part::Field("selectedGCode", "jobs/leg.nc")]).await.status, 200);
    assert_eq!(server.post_json("/api/queue/add", json!({"file": "queued/a.nc"})).await.status, 200);
    for path in ["jobs/leg.nc", "jobs", "./jobs/", "queued/a.nc", "queued"] {
        let reply = server.post_json("/api/files/gcode/delete", json!({"path": path})).await;
        assert_eq!(reply.status, StatusCode::CONFLICT, "{path}");
        assert!(reply.json()["error"].as_str().unwrap().contains("the loaded gcode or a file in the job queue"));
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(server.dir().join("gcode/jobs"), server.dir().join("gcode/link")).unwrap();
        let reply = server.post_json("/api/files/gcode/delete", json!({"path": "link/leg.nc"})).await;
        assert_eq!(reply.status, StatusCode::CONFLICT);
    }
    assert!(server.dir().join("gcode/jobs/leg.nc").is_file() && server.dir().join("gcode/queued/a.nc").is_file());
    assert_eq!(server.post_json("/api/files/gcode/delete", json!({"path": "jobs2"})).await.status, StatusCode::OK);
}

#[tokio::test]
async fn a_delete_stays_inside_the_library_and_below_its_top_level() {
    let server = Server::new();
    server.write("precious.txt", "keep");
    for (path, status) in [
        ("../precious.txt", StatusCode::BAD_REQUEST),
        ("/etc", StatusCode::BAD_REQUEST),
        (".", StatusCode::BAD_REQUEST),
        ("", StatusCode::BAD_REQUEST),
        ("missing.nc", StatusCode::NOT_FOUND),
    ] {
        assert_eq!(server.post_json("/api/files/gcode/delete", json!({"path": path})).await.status, status, "{path}");
    }
    assert!(server.dir().join("precious.txt").is_file() && server.dir().join("gcode").is_dir());
    let reply = server.post_raw("/api/files/gcode/delete", "application/json", br#"{"name": "x"}"#).await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("a.nc"), "").unwrap();
        std::os::unix::fs::symlink(outside.path(), server.dir().join("gcode/out")).unwrap();
        let reply = server.post_json("/api/files/gcode/delete", json!({"path": "out/a.nc"})).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST);
        assert!(outside.path().join("a.nc").is_file());
    }
}

/// Upstream #51: View/Edit G-code line N, Go to line N (which sends N - 1) and the Line readout are the same line.
#[tokio::test]
async fn go_to_line_n_shows_the_editors_line_n() {
    let server = Server::new();
    server.write("gcode/numbered.nc", "\u{feff}(header)\r\n\r\nG21\n\n\n; note\nG0 X1\n\n(cut)\nG1 X2\nG1 X3\n");
    server.post_form("/openGCode", &[Part::Field("selectedGCode", "numbered.nc")]).await;
    let editor = server.get("/api/files/gcode/text").await.json()["text"].as_str().unwrap().to_string();
    for (at, text) in editor.lines().enumerate() {
        let line = at + 1;
        server.act("moveGcodeGoto", json!(line - 1));
        server.settle().await;
        let shown = server.get("/api/main").await.json()["gcode"].clone();
        assert_eq!(
            (shown["lineIndex"].as_u64().unwrap() as usize + 1, shown["line"].as_str().unwrap().trim_end()),
            (line, text.trim_end())
        );
    }
}

#[tokio::test]
async fn opening_a_listed_file_loads_it_and_names_where_it_came_from() {
    let server = Server::new();
    server.write("gcode/jobs/leg.nc", "G21\r\nG0 X1 Y1\r\nG1 Z-1\r\n");
    let reply = server.post_form("/openGCode", &[Part::Field("selectedGCode", "jobs/leg.nc")]).await;
    assert_eq!((reply.status, reply.json()), (StatusCode::OK, json!({"status": 200})));
    let loaded = server.get("/api/files/gcode/loaded").await.json();
    assert_eq!(loaded, json!({"directory": "jobs", "name": "leg.nc", "lineCount": 4}));
    assert_eq!(server.get("/api/files/gcode/text").await.json(), json!({"text": "G21 \nG0 X1 Y1 \nG1 Z-1"}));
    server.settle().await;
    let saved = server.saved_settings();
    assert_eq!(setting(&saved, "Computed Settings", "lastSelectedDirectory"), "jobs");
    assert_eq!(setting(&saved, "Maslow Settings", "openFile"), "leg.nc");
    let viewer = server.get("/api/viewer").await.json();
    assert!(viewer["gcode"].as_str().is_some_and(|g| g.starts_with("H4sI")));
}

#[tokio::test]
async fn the_last_run_is_drawn_from_its_file_until_the_file_is_gone() {
    let server = Server::new();
    assert_eq!(server.get("/api/viewer/last-run").await.json(), Value::Null);
    server.write("gcode/square.nc", SQUARE);
    let file = server.dir().join("gcode/square.nc");
    let changes = vec![
        ("Computed Settings".into(), "lastRunFile".into(), json!(file.to_string_lossy())),
        ("Computed Settings".into(), "lastRunHomeX".into(), json!(10.0)),
        ("Computed Settings".into(), "lastRunHomeY".into(), json!(-5.0)),
    ];
    server.machine.request(wc_machine::files::Request::SetSettings(changes)).await;
    let last = server.get("/api/viewer/last-run").await.json();
    assert!(last["gcode"].as_str().is_some_and(|g| g.starts_with("H4sI")));
    assert_eq!((last["homeX"].clone(), last["homeY"].clone()), (json!(10.0), json!(-5.0)));
    std::fs::remove_file(file).unwrap();
    assert_eq!(server.get("/api/viewer/last-run").await.json(), Value::Null);
}

#[tokio::test]
async fn every_client_hears_which_file_is_loaded_and_the_state_names_it() {
    let server = Server::new();
    assert_eq!(server.get("/api/state").await.json()["gcodeFile"], Value::Null);
    server.write("gcode/mancala/mancala.frd.roughing.25.endmill.nc", "G21\nG0 X1 Y1\n");
    let mut pushes = server.machine.subscribe();
    let selected = Part::Field("selectedGCode", "mancala/mancala.frd.roughing.25.endmill.nc");
    assert_eq!(server.post_form("/openGCode", &[selected]).await.status, StatusCode::OK);
    server.settle().await;
    let named = json!("{\"file\":\"mancala/mancala.frd.roughing.25.endmill.nc\"}");
    assert_eq!(drain(&mut pushes, "gcodeFile"), [named]);
    let state = server.get("/api/state").await.json();
    assert_eq!(state["gcodeFile"], "mancala/mancala.frd.roughing.25.endmill.nc");
    server.machine.send(Event::Action(Action::new("clearGCode", json!(""), json!(""))));
    server.settle().await;
    assert_eq!(drain(&mut pushes, "gcodeFile"), [json!("{\"file\":null}")]);
    assert_eq!(server.get("/api/state").await.json()["gcodeFile"], Value::Null);
}

#[tokio::test]
async fn opening_needs_a_directory_and_a_readable_parsable_file() {
    let server = Server::new();
    server.write("gcode/top.nc", "G0 X1\n");
    assert_eq!(server.post_form("/openGCode", &[Part::Field("selectedGCode", "top.nc")]).await.status, 500);
    assert_eq!(server.post_form("/openGCode", &[Part::Field("selectedGCode", "./missing.nc")]).await.status, 500);
    let reply = server.post_unchecked("/openGCode", &[]).await;
    assert_eq!((reply.status, reply.content_type.starts_with("text/html")), (StatusCode::BAD_REQUEST, true));
    let truncating = machine_with({
        let mut settings = Settings::defaults();
        settings.set_value("Advanced Settings", "truncate", json!(1));
        settings.set_value("Advanced Settings", "digits", json!(4));
        settings
    });
    let server = Server::with(truncating);
    server.write("gcode/a.nc", "G0 X1.123456\n");
    let mut pushes = server.machine.subscribe();
    assert_eq!(server.post_form("/openGCode", &[Part::Field("selectedGCode", "./a.nc")]).await.status, 500);
    server.settle().await;
    let alerts = drain(&mut pushes, "activateModal");
    assert!(alerts[0].as_str().unwrap().contains("Cannot open gcode file."), "{alerts:?}");
}

/// Ported from test_safe_paths.
#[tokio::test]
async fn paths_outside_the_libraries_are_refused() {
    let server = Server::new();
    for selected in ["../webcontrol.json", "./../webcontrol.json", "/etc/hosts", "%2e%2e/webcontrol.json"] {
        let reply = server.post_form("/openGCode", &[Part::Field("selectedGCode", selected)]).await;
        assert_eq!(reply.json(), json!({"status": 400, "error": "Path is outside the library"}), "{selected}");
        let board = server.post_form("/openBoard", &[Part::Field("selectedBoard", selected)]).await;
        assert_eq!(board.status, StatusCode::BAD_REQUEST, "{selected}");
    }
    for (directory, name) in [("..", "evil.nc"), (".", "../evil.nc"), ("/tmp", "evil.nc")] {
        let fields = [Part::Field("fileName", name), Part::Field("selectedDirectory", directory)];
        assert_eq!(server.post_form("/saveGCode", &fields).await.status, StatusCode::BAD_REQUEST);
        assert_eq!(server.post_form("/saveBoard", &fields).await.status, StatusCode::BAD_REQUEST);
    }
    let upload = [Part::Field("selectedDirectory", ".."), Part::File("file[]", "evil.nc", b"G0 X0\n")];
    assert_eq!(server.post_form("/uploadGCode", &upload).await.status, StatusCode::BAD_REQUEST);
    assert!(!server.home.path().join("evil.nc").exists());
}

#[tokio::test]
async fn uploading_saves_every_file_and_opens_the_first() {
    let server = Server::new();
    let parts = [
        Part::Field("selectedDirectory", "jobs"),
        Part::File("file[]", "My Part.nc", SQUARE.as_bytes()),
        Part::File("file[]", "second.nc", b"G0 X9\n"),
    ];
    std::fs::create_dir_all(server.dir().join("gcode/jobs")).unwrap();
    let reply = server.post_form("/uploadGCode", &parts).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert!(server.dir().join("gcode/jobs/My_Part.nc").is_file());
    assert!(server.dir().join("gcode/jobs/second.nc").is_file());
    let mut saved: Vec<_> =
        std::fs::read_dir(server.dir().join("gcode/jobs")).unwrap().map(|e| e.unwrap().file_name()).collect();
    saved.sort();
    assert_eq!(saved, ["My_Part.nc", "second.nc"], "no partial upload is left beside them");
    let loaded = server.get("/api/files/gcode/loaded").await.json();
    assert_eq!(loaded, json!({"directory": "jobs", "name": "My_Part.nc", "lineCount": 10}));
    let none = server.post_unchecked("/uploadGCode", &[Part::Field("selectedDirectory", ".")]).await;
    assert_eq!(none.status, StatusCode::INTERNAL_SERVER_ERROR);
    let unwritable = [Part::Field("selectedDirectory", "missing"), Part::File("file[]", "a.nc", b"G0\n")];
    assert_eq!(server.post_form("/uploadGCode", &unwritable).await.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(server.post_unchecked("/uploadGCode", &[]).await.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn saving_writes_the_loaded_lines_into_the_library() {
    let server = Server::new();
    server.write("gcode/part.nc", "G0 X1\nG0 X2");
    server.post_form("/openGCode", &[Part::Field("selectedGCode", "./part.nc")]).await;
    let fields = [Part::Field("fileName", "copy.nc"), Part::Field("selectedDirectory", "./")];
    assert_eq!(server.post_form("/saveGCode", &fields).await.json(), json!({"status": 200}));
    assert_eq!(std::fs::read_to_string(server.dir().join("gcode/copy.nc")).unwrap(), "G0 X1 \nG0 X2 \n");
    server.settle().await;
    assert_eq!(setting(&server.saved_settings(), "Maslow Settings", "openFile"), "copy.nc");
    let mut pushes = server.machine.subscribe();
    let missing = [Part::Field("fileName", "x.nc"), Part::Field("selectedDirectory", "nowhere")];
    assert_eq!(server.post_form("/saveGCode", &missing).await.status, StatusCode::INTERNAL_SERVER_ERROR);
    server.settle().await;
    assert!(drain(&mut pushes, "activateModal")[0].as_str().unwrap().contains("Cannot save gcode file."));
    assert_eq!(server.get("/api/files/gcode/loaded").await.json()["name"], Value::Null, "Python forgets the file");
    assert_eq!(server.post_unchecked("/saveGCode", &[Part::Field("fileName", "x")]).await.status, 400);
}

#[tokio::test]
async fn editing_replaces_the_lines_and_an_empty_edit_reads_the_file_again() {
    let server = Server::new();
    server.write("gcode/part.nc", "G0 X1\nG0 X2\n");
    server.post_form("/openGCode", &[Part::Field("selectedGCode", "./part.nc")]).await;
    let reply = server.post_form("/editGCode", &[Part::Field("gcode", "G0 X5\nG0 X6\n\n")]).await;
    assert_eq!((reply.status, reply.json()), (StatusCode::OK, json!("success")));
    assert_eq!(server.get("/api/files/gcode/text").await.json()["text"], "G0 X5 \nG0 X6");
    assert_eq!(server.get("/api/files/gcode/loaded").await.json()["name"], "part.nc", "an edit keeps the name");
    server.post_form("/editGCode", &[Part::Field("gcode", "  \n")]).await;
    assert_eq!(server.get("/api/files/gcode/text").await.json()["text"], "G0 X1 \nG0 X2", "read from disk again");
    std::fs::remove_file(server.dir().join("gcode/part.nc")).unwrap();
    let reply = server.post_form("/editGCode", &[Part::Field("gcode", "")]).await;
    assert_eq!(reply.json(), json!("success"), "Python answers success after its alert");
    let fresh = Server::new();
    assert_eq!(fresh.post_form("/editGCode", &[Part::Field("gcode", "")]).await.json(), json!("success"));
    assert_eq!(fresh.get("/api/files/gcode/loaded").await.json()["lineCount"], 0);
    assert_eq!(fresh.post_unchecked("/editGCode", &[]).await.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn an_edit_python_cannot_load_alerts_but_answers_success() {
    let mut settings = Settings::defaults();
    settings.set_value("Advanced Settings", "truncate", json!(1));
    settings.set_value("Advanced Settings", "digits", json!("x"));
    let server = Server::with(machine_with(settings));
    let mut pushes = server.machine.subscribe();
    assert_eq!(server.post_form("/editGCode", &[Part::Field("gcode", "G0 X1")]).await.json(), json!("success"));
    server.settle().await;
    assert!(drain(&mut pushes, "activateModal")[0].as_str().unwrap().contains("Cannot open gcode file."));
}

const STILL_LOADING: &str = "Gcode is still loading";

/// Presses Play on the unconnected machine and answers whether it was refused for a load underway.
async fn play_waits_for_a_load(server: &Server, pushes: &mut tokio::sync::broadcast::Receiver<Push>) -> bool {
    server.act("startRun", json!(""));
    server.settle().await;
    drain(pushes, "activateModal").iter().any(|alert| alert.as_str().unwrap().contains(STILL_LOADING))
}

async fn next_preview(pushes: &mut tokio::sync::broadcast::Receiver<Push>) {
    let preview = async { while pushes.recv().await.unwrap().data["command"] != "gcodeUpdateCompressed" {} };
    tokio::time::timeout(Duration::from_secs(5), preview).await.expect("the preview was sent");
}

/// Writes `text` into the FIFO a load is reading, which ends the read.
async fn release(path: std::path::PathBuf, text: &'static str) {
    tokio::task::spawn_blocking(move || std::fs::write(path, text)).await.unwrap().unwrap();
}

#[tokio::test]
async fn a_load_whose_client_leaves_still_finishes_sends_the_preview_and_frees_play() {
    let server = Server::new();
    let path = server.dir().join("gcode/slow.nc");
    support::fifo(&path);
    let mut pushes = server.machine.subscribe();
    let open = server.post_form("/openGCode", &[Part::Field("selectedGCode", "./slow.nc")]);
    assert!(tokio::time::timeout(Duration::from_millis(300), open).await.is_err(), "the read waits on the FIFO");
    assert!(play_waits_for_a_load(&server, &mut pushes).await);
    release(path, SQUARE).await;
    next_preview(&mut pushes).await;
    let loaded = server.get("/api/files/gcode/loaded").await.json();
    assert_eq!(loaded, json!({"directory": ".", "name": "slow.nc", "lineCount": 10}));
    assert!(!play_waits_for_a_load(&server, &mut pushes).await);
}

#[tokio::test]
async fn a_failed_load_frees_play() {
    let server = Server::new();
    let mut pushes = server.machine.subscribe();
    assert_eq!(server.post_form("/openGCode", &[Part::Field("selectedGCode", "./missing.nc")]).await.status, 500);
    assert!(!play_waits_for_a_load(&server, &mut pushes).await);
}

#[tokio::test]
async fn a_second_load_waits_its_turn_and_the_later_file_stays_loaded() {
    let server = Server::new();
    let path = server.dir().join("gcode/slow.nc");
    support::fifo(&path);
    server.write("gcode/quick.nc", "G0 X1\n");
    let mut pushes = server.machine.subscribe();
    let slow = server.post_form("/openGCode", &[Part::Field("selectedGCode", "./slow.nc")]);
    let quick = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        server.post_form("/openGCode", &[Part::Field("selectedGCode", "./quick.nc")]).await
    };
    let queued = async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(server.get("/api/files/gcode/loaded").await.json()["name"], Value::Null);
        assert!(play_waits_for_a_load(&server, &mut pushes).await);
        release(path, SQUARE).await;
    };
    let (slow, quick, ()) = tokio::join!(slow, quick, queued);
    assert_eq!((slow.status, quick.status), (StatusCode::OK, StatusCode::OK));
    assert_eq!(server.get("/api/files/gcode/loaded").await.json()["name"], "quick.nc");
    assert!(!play_waits_for_a_load(&server, &mut pushes).await);
}

/// A machine streaming a job, so the gcode is locked.
fn streaming() -> Machine {
    let mut machine = machine_with(Settings::defaults());
    let now = Duration::ZERO;
    machine.handle_inline(Event::PortOpened { port: "/dev/ttyACM0".into() }, now);
    machine.load_gcode("G0 X1\nG1 Z-1\n");
    machine.handle_inline(Event::Action(Action::new("startRun", json!(""), json!(""))), now);
    machine
}

/// Ported from test_gcode_load_guard.
#[tokio::test]
async fn the_gcode_cannot_be_replaced_during_a_job() {
    let server = Server::with(streaming());
    let message = "Cannot load gcode while a job is running or paused. Stop the job first.";
    let forms: [(&str, Vec<Part>); 3] = [
        ("/openGCode", vec![Part::Field("selectedGCode", "./other.nc")]),
        ("/uploadGCode", vec![Part::Field("selectedDirectory", "."), Part::File("file[]", "guarded.nc", b"G0 X9\n")]),
        ("/editGCode", vec![Part::Field("gcode", "G0 X9")]),
    ];
    for (path, parts) in &forms {
        let reply = server.post_form(path, parts).await;
        assert_eq!((reply.status, reply.json()), (StatusCode::CONFLICT, json!({"status": 409, "error": message})));
    }
    assert!(!server.dir().join("gcode/guarded.nc").exists());
    assert_eq!(server.get("/api/files/gcode/text").await.json()["text"], "G0 X1 \nG1 Z-1");
    let sent = server.post_form("/sendGCode", &[Part::Field("gcode", "G0 X1")]).await;
    assert_eq!(sent.status, StatusCode::CONFLICT, "decision 2, where Python sends it mid-job");
}

/// Ported from test_gcode_load_guard and test_job_in_progress: a paused job, or a tool change outside one, locks too.
#[tokio::test]
async fn the_gcode_stays_locked_while_a_job_is_paused_or_a_tool_change_waits() {
    let mut paused = streaming();
    paused.handle_inline(Event::Action(Action::new("pauseRun", json!(""), json!(""))), Duration::ZERO);
    let mut tool_change = machine_with(Settings::defaults());
    tool_change.handle_inline(Event::PortOpened { port: "/dev/ttyACM0".into() }, Duration::ZERO);
    tool_change.handle_inline(Event::ControllerLine("Tool Change: Please insert tool 2".into()), Duration::ZERO);
    for machine in [paused, tool_change] {
        let server = Server::with(machine);
        let reply = server.post_form("/editGCode", &[Part::Field("gcode", "G0 X9")]).await;
        assert_eq!(reply.status, StatusCode::CONFLICT);
    }
}

#[tokio::test]
async fn sent_gcode_is_remembered_and_refused_while_probing() {
    let server = Server::new();
    assert_eq!(server.get("/api/files/gcode/custom").await.json(), json!({"text": ""}));
    let reply = server.post_form("/sendGCode", &[Part::Field("gcode", "G0 X5\nG0 Y5\n")]).await;
    assert_eq!(reply.json(), json!("success"));
    assert_eq!(server.get("/api/files/gcode/custom").await.json(), json!({"text": "G0 X5\nG0 Y5"}));
    assert_eq!(server.post_unchecked("/sendGCode", &[]).await.status, StatusCode::BAD_REQUEST);
    let mut probing = machine_with(Settings::defaults());
    probing.handle_inline(Event::PortOpened { port: "p".into() }, Duration::ZERO);
    probing.handle_inline(Event::Action(Action::new("touchZ", json!(""), json!(""))), Duration::ZERO);
    let server = Server::with(probing);
    let reply = server.post_form("/sendGCode", &[Part::Field("gcode", "G0 X1")]).await;
    assert_eq!((reply.status, reply.json()), (StatusCode::INTERNAL_SERVER_ERROR, json!("failed")));
    assert_eq!(server.get("/api/state").await.json()["probing"], true);
    assert_eq!(server.get("/api/main").await.json()["status"]["probing"], true);
}

#[tokio::test]
async fn url_encoded_forms_work_too_and_garbled_bodies_are_bad_requests() {
    let server = Server::new();
    let reply = server.post_raw("/sendGCode", "application/x-www-form-urlencoded", b"gcode=G0+X1%0AG0+X2").await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(server.get("/api/files/gcode/custom").await.json()["text"], "G0 X1\nG0 X2");
    let garbled = server.post_raw("/sendGCode", "multipart/form-data; boundary=x", b"--y\r\nnonsense").await;
    assert_eq!(garbled.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn the_viewer_and_current_board_follow_the_board() {
    let server = Server::new();
    let board = server.get("/api/boards/current").await.json();
    assert_eq!(
        board,
        json!({"boardID": "-", "material": "-", "width": 96.0, "height": 48.0, "thickness": 0.75, "centerX": 0.0,
            "centerY": 0.0, "units": "inches", "fileName": "", "router": {"x": 0.0, "y": 0.0}})
    );
    let form = [
        Part::Field("boardID", "Shelf"),
        Part::Field("material", "Plywood"),
        Part::Field("width", "48"),
        Part::Field("height", "24"),
        Part::Field("thickness", "0.75"),
        Part::Field("centerX", "1.5"),
        Part::Field("centerY", "-2"),
        Part::Field("units", "inches"),
    ];
    assert_eq!(server.post_form("/editBoard", &form).await.json(), json!("success"));
    let viewer = server.get("/api/viewer").await.json();
    assert_eq!(viewer["board"]["boardID"], "Shelf");
    let metric = {
        let mut settings = Settings::defaults();
        settings.set_value("Computed Settings", "units", json!("MM"));
        Server::with(machine_with(settings))
    };
    metric.post_form("/editBoard", &form).await;
    let body = metric.get("/api/boards/current").await.json();
    assert_eq!(
        (body["units"].clone(), body["width"].clone(), body["thickness"].clone()),
        (json!("mm"), json!(1219.2), json!(19.05))
    );
    assert_eq!((body["centerX"].clone(), body["centerY"].clone()), (json!(38.1), json!(-50.8)));
}

#[tokio::test]
async fn a_board_edit_missing_a_field_or_a_number_fails() {
    let server = Server::new();
    assert_eq!(server.post_unchecked("/editBoard", &[Part::Field("boardID", "x")]).await.json(), json!("failed"));
    let form = ["boardID", "material", "width", "height", "thickness", "centerX", "centerY"]
        .map(|name| Part::Field(name, if name == "width" { "wide" } else { "1" }));
    let mut parts: Vec<Part> = form.into_iter().collect();
    parts.push(Part::Field("units", "mm"));
    assert_eq!(server.post_form("/editBoard", &parts).await.status, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn a_board_whose_size_is_not_a_number_cannot_be_reported() {
    let server = Server::new();
    server.write(
        "boards/odd.brd",
        "{\"width\": \"wide\", \"height\": \"2\", \"thickness\": 1, \"centerX\": 0, \
        \"centerY\": 0, \"boardID\": \"B\", \"material\": \"M\", \"fileName\": \"odd.brd\"}\n",
    );
    assert_eq!(server.post_form("/openBoard", &[Part::Field("selectedBoard", "./odd.brd")]).await.status, 200);
    let reply = server.call("GET", "/api/boards/current", None, Vec::new()).await;
    assert_eq!(reply.status, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn boards_are_trimmed_saved_listed_and_opened() {
    let server = Server::new();
    let trim = [
        Part::Field("trimTop", "0"),
        Part::Field("trimBottom", "0"),
        Part::Field("trimLeft", "6"),
        Part::Field("trimRight", "0"),
        Part::Field("units", "inches"),
    ];
    assert_eq!(server.post_form("/trimBoard", &trim).await.json(), json!("success"));
    assert_eq!(server.get("/api/boards/current").await.json()["width"], 90.0);
    let bad = [
        Part::Field("trimTop", "x"),
        Part::Field("trimBottom", "0"),
        Part::Field("trimLeft", "0"),
        Part::Field("trimRight", "0"),
        Part::Field("units", "mm"),
    ];
    assert_eq!(server.post_form("/trimBoard", &bad).await.json(), json!("failed"));
    assert_eq!(server.post_unchecked("/trimBoard", &[Part::Field("trimTop", "1")]).await.status, 400);
    std::fs::create_dir_all(server.dir().join("boards/shop")).unwrap();
    let save = [Part::Field("fileName", "bench.brd"), Part::Field("selectedDirectory", "shop")];
    assert_eq!(server.post_form("/saveBoard", &save).await.json(), json!({"status": 200}));
    let file = std::fs::read_to_string(server.dir().join("boards/shop/bench.brd")).unwrap();
    assert!(file.starts_with("{\"width\": 90.0, ") && file.contains("\"fileName\": \"bench.brd\""), "{file}");
    let library = server.get("/api/boards/files").await.json();
    assert_eq!(library["directories"], json!([".", "shop"]));
    assert_eq!(library["files"], json!([{"directory": "shop", "name": "bench.brd"}]));
    assert_eq!((library["lastDirectory"].clone(), library["lastFile"].clone()), (json!("shop"), json!("bench.brd")));
    assert_eq!(server.post_form("/openBoard", &[Part::Field("selectedBoard", "shop/bench.brd")]).await.status, 200);
    server.settle().await;
    let saved = server.saved_settings();
    assert_eq!(setting(&saved, "Maslow Settings", "openBoardFile"), "bench.brd");
    assert_eq!(server.post_json("/api/boards/directories", json!({"name": "garage"})).await.status, 201);
    assert_eq!(server.post_json("/api/boards/directories", json!({"name": ""})).await.status, 400);
    assert_eq!(server.post_unchecked("/openBoard", &[]).await.status, 400);
    assert_eq!(server.post_unchecked("/saveBoard", &[]).await.status, 400);
}

#[tokio::test]
async fn the_board_library_falls_back_to_the_open_board_setting() {
    let server = Server::new();
    let change = vec![("Maslow Settings".into(), "openBoardFile".into(), json!("old.brd"))];
    server.machine.request(wc_machine::files::Request::SetSettings(change)).await;
    assert_eq!(server.get("/api/boards/files").await.json()["lastFile"], "old.brd");
}

#[tokio::test]
async fn boards_that_cannot_be_read_or_saved_fail() {
    let server = Server::new();
    assert_eq!(server.post_form("/openBoard", &[Part::Field("selectedBoard", "./missing.brd")]).await.status, 500);
    server.write("boards/junk.brd", "not json\n");
    assert_eq!(server.post_form("/openBoard", &[Part::Field("selectedBoard", "./junk.brd")]).await.status, 500);
    let nowhere = [Part::Field("fileName", "x.brd"), Part::Field("selectedDirectory", "nowhere")];
    assert_eq!(server.post_form("/saveBoard", &nowhere).await.status, 500);
    server.act("boardClearBoard", json!(""));
    let after_clear = [Part::Field("fileName", "x.brd"), Part::Field("selectedDirectory", ".")];
    assert_eq!(server.post_form("/saveBoard", &after_clear).await.status, 500, "Python has no cut data to save");
}

/// Ported from test_api_boards: opening a board leaves the loaded gcode alone.
#[tokio::test]
async fn opening_a_board_keeps_the_loaded_gcode_name() {
    let server = Server::new();
    server.write("gcode/leg.nc", "G0 X0\nG1 X1");
    server.post_form("/openGCode", &[Part::Field("selectedGCode", "./leg.nc")]).await;
    server.write(
        "boards/shelf.brd",
        "{\"width\": 4, \"height\": 2, \"thickness\": 1, \"centerX\": 0, \"centerY\": 0, \
        \"boardID\": \"S\", \"material\": \"M\", \"fileName\": \"shelf.brd\"}\n",
    );
    server.post_form("/openBoard", &[Part::Field("selectedBoard", "./shelf.brd")]).await;
    assert_eq!(
        server.get("/api/files/gcode/loaded").await.json(),
        json!({"directory": ".", "name": "leg.nc", "lineCount": 2})
    );
}

#[test]
fn the_loaded_file_is_described_relative_to_the_library() {
    let root = Path::new("/home/me/.WebControl/gcode");
    let describe = |file: &str| wc_server::files::loaded(2, Some(Path::new(file)), root);
    let at = |file: &str| {
        let loaded = describe(file);
        (loaded.directory, loaded.name)
    };
    assert_eq!(at("/home/me/.WebControl/gcode/./part.nc"), (Some(".".into()), Some("part.nc".into())));
    assert_eq!(at("/home/me/.WebControl/gcode/jobs/part.nc"), (Some("jobs".into()), Some("part.nc".into())));
    assert_eq!(at("/home/me/.WebControl/gcode/a/b/part.nc"), (None, Some("part.nc".into())));
    assert_eq!(at("/elsewhere/part.nc"), (None, Some("part.nc".into())));
    assert_eq!(wc_server::files::loaded(0, Some(Path::new("/x.nc")), root).name, None);
    assert_eq!(wc_server::files::loaded(3, None, root).line_count, 3);
}

/// `SQUARE` is 9 lines, about 3 kB at 350 bytes a line.
fn tight() -> Option<u64> {
    Some(5_000)
}

fn full() -> Option<u64> {
    Some(3_000)
}

#[tokio::test]
async fn a_file_that_strains_free_memory_loads_with_a_warning() {
    let server = Server::freeing(machine_with(Settings::defaults()), tight);
    let parts = [Part::Field("selectedDirectory", "."), Part::File("file[]", "square.nc", SQUARE.as_bytes())];
    let reply = server.post_form("/uploadGCode", &parts).await.json();
    assert_eq!(reply["status"], 200);
    let warning = reply["warning"].as_str().unwrap();
    assert!(warning.starts_with("This file has 9 lines and needs about 1 MB of the 0 MB"), "{warning}");
    assert!(warning.ends_with("running them one after another with the job queue."), "{warning}");
    assert_eq!(server.get("/api/files/gcode/loaded").await.json()["lineCount"], 10);
}

#[tokio::test]
async fn a_file_too_large_for_free_memory_is_refused() {
    let server = Server::freeing(machine_with(Settings::defaults()), full);
    server.write("gcode/square.nc", SQUARE);
    let reply = server.post_form("/openGCode", &[Part::Field("selectedGCode", "./square.nc")]).await;
    assert_eq!(reply.status, StatusCode::CONFLICT);
    let error = reply.json()["error"].as_str().unwrap().to_string();
    assert!(error.contains("It was not opened. Try splitting it into smaller files"), "{error}");
    assert_eq!(server.get("/api/files/gcode/loaded").await.json()["lineCount"], 0);
}
