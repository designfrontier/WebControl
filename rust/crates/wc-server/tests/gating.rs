//! Decision 2: while a job holds the gcode, the settings writes, imports, restore and `/sendGCode` answer 409.

mod support;

use std::time::Duration;

use axum::http::StatusCode;
use serde_json::{Value, json};
use support::{Part, Server, machine_with};
use wc_config::Settings;
use wc_machine::files::{SEND_LOCKED, SETTINGS_LOCKED};
use wc_machine::{Event, Machine};
use wc_protocol::Action;

fn streaming() -> Machine {
    let mut machine = machine_with(Settings::defaults());
    machine.handle(Event::PortOpened { port: "/dev/ttyACM0".into() }, Duration::ZERO);
    machine.load_gcode("G0 X1\nG1 Z-1\n");
    machine.handle(Event::Action(Action::new("startRun", json!(""), json!(""))), Duration::ZERO);
    machine
}

fn backup_zip() -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let entries = [
        ("webcontrol.json", "{\"Maslow Settings\": [{\"key\": \"COMport\", \"value\": \"/dev/ttyRESTORED\"}]}"),
        ("gcode/restored.nc", "G0 X0\n"),
    ];
    entries.iter().for_each(|(name, text)| {
        zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        std::io::Write::write_all(&mut zip, text.as_bytes()).unwrap();
    });
    zip.finish().unwrap().into_inner()
}

#[tokio::test]
async fn settings_writes_are_refused_during_a_job() {
    let server = Server::with(streaming());
    let refused = json!({"error": SETTINGS_LOCKED});
    let port = json!({"values": {"COMport": "/dev/ttyUSB0"}});
    let reply = server.send_json("PUT", "/api/settings/maslow", port).await;
    assert_eq!((reply.status, reply.json()), (StatusCode::CONFLICT, refused.clone()));
    let quick: Value = server.get("/api/machine/quick-configure").await.json();
    let reply = server.post_json("/api/machine/quick-configure", quick).await;
    assert_eq!((reply.status, reply.json()), (StatusCode::CONFLICT, refused));
    assert_eq!(server.machine.snapshot().settings.text_or_empty("Maslow Settings", "COMport"), "");
}

#[tokio::test]
async fn sending_gcode_is_refused_during_a_job() {
    let server = Server::with(streaming());
    let reply = server.post_form("/sendGCode", &[Part::Field("gcode", "G0 X9")]).await;
    assert_eq!((reply.status, reply.json()), (StatusCode::CONFLICT, json!({"status": 409, "error": SEND_LOCKED})));
    assert_eq!(server.get("/api/files/gcode/custom").await.json()["text"], "");
}

#[tokio::test]
async fn imports_and_restores_are_refused_during_a_job_before_anything_is_written() {
    let server = Server::with(streaming());
    let before = server.saved_settings();
    let backup = backup_zip();
    let uploads: [(&str, &str, &[u8]); 3] = [
        ("/importFile", "groundcontrol.ini", b"[Maslow Settings]\ncomport = /dev/ttyINI\n"),
        ("/importFileWCJSON", "webcontrol.json", br#"{"Maslow Settings": [{"key": "COMport", "value": "x"}]}"#),
        ("/importRestoreWebControl", "wc_backup.zip", &backup),
    ];
    for (path, name, bytes) in uploads {
        let reply = server.post_form(path, &[Part::File("file", name, bytes)]).await;
        let refused = json!({"status": 409, "error": SETTINGS_LOCKED});
        assert_eq!((reply.status, reply.json()), (StatusCode::CONFLICT, refused), "{path}");
    }
    assert!(!server.dir().join("gcode/restored.nc").exists());
    assert!(!server.home.path().join("wc_backup.zip").exists());
    assert!(!server.dir().join("imports/groundcontrol.ini").exists());
    assert_eq!(server.saved_settings(), before);
    assert_eq!(server.machine.snapshot().settings.text_or_empty("Maslow Settings", "COMport"), "");
}

#[tokio::test]
async fn the_lock_lifts_when_the_job_is_stopped() {
    let server = Server::with(streaming());
    server.act("stopRun", json!(""));
    server.settle().await;
    let reply = server.post_form("/sendGCode", &[Part::Field("gcode", "G0 X9")]).await;
    assert_eq!((reply.status, reply.json()), (StatusCode::OK, json!("success")));
    let reply = server.send_json("PUT", "/api/settings/maslow", json!({"values": {"motorSpacingX": "3000"}})).await;
    assert_eq!(reply.status, StatusCode::OK);
}

#[tokio::test]
async fn the_line_pacing_changes_during_a_job_and_takes_effect_at_once() {
    let server = Server::with(streaming());
    let body = json!({"values": {"linePacingMode": "minimum gap", "linePacingGapMs": 25}});
    let reply = server.send_json("PUT", "/api/run/pacing", body).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!((reply.json()["mode"].clone(), reply.json()["gapMs"].clone()), (json!("minimum gap"), json!(25)));
    assert_eq!(reply.json()["history"][0]["endedBy"], "pacing change");
    let pacing = server.get("/api/run/pacing").await.json();
    assert_eq!(
        (pacing["segment"]["mode"].clone(), pacing["segment"]["gapMs"].clone()),
        (json!("minimum gap"), json!(25))
    );
    server.settle().await;
    let saved = server.saved_settings();
    let webcontrol = saved["WebControl Settings"].as_array().unwrap();
    let value = |key: &str| webcontrol.iter().find(|option| option["key"] == key).unwrap()["value"].clone();
    assert_eq!((value("linePacingMode"), value("linePacingGapMs")), (json!("minimum gap"), json!(25)));
}
