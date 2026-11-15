//! Backups, diagnostics, restores and the settings imports (ported from test_restore and test_log_retention).

mod support;

use std::io::{Read, Write};
use std::path::Path;

use axum::http::StatusCode;
use serde_json::{Value, json};
use support::{Part, Server};
use wc_server::archive::{backup, diagnostics, restore, timestamp};

fn names(archive: &Path) -> Vec<String> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(archive).unwrap()).unwrap();
    let mut names: Vec<String> = (0..zip.len()).map(|i| zip.by_index(i).unwrap().name().unwrap().to_string()).collect();
    names.sort();
    names
}

fn zip_of(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    entries.iter().for_each(|(name, text)| {
        zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    });
    zip.finish().unwrap().into_inner()
}

#[test]
fn timestamps_are_pythons_strftime() {
    let stamp = timestamp(1_700_000_000);
    assert_eq!(stamp.len(), 15);
    assert!(stamp.starts_with("2023111"), "{stamp}");
    assert_eq!(stamp.as_bytes()[8], b'-');
}

#[test]
fn a_backup_zips_the_whole_webcontrol_directory_into_home() {
    let home = tempfile::tempdir().unwrap();
    let dir = home.path().join(".WebControl");
    std::fs::create_dir_all(dir.join("gcode/jobs")).unwrap();
    std::fs::write(dir.join("webcontrol.json"), "{}").unwrap();
    std::fs::write(dir.join("gcode/jobs/leg.nc"), "G0 X1\n").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(home.path(), dir.join("loop")).unwrap();
    let archive = backup(home.path()).unwrap();
    assert_eq!(archive.parent().unwrap(), home.path());
    assert!(archive.file_name().unwrap().to_str().unwrap().starts_with("wc_backup_"));
    assert_eq!(names(&archive), ["gcode/jobs/leg.nc", "webcontrol.json"]);
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&archive).unwrap()).unwrap();
    assert_eq!(zip.by_name("gcode/jobs/leg.nc").unwrap().compression(), zip::CompressionMethod::Deflated);
}

#[test]
fn only_the_newest_five_backups_are_kept() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".WebControl")).unwrap();
    (0..7).for_each(|i| std::fs::write(home.path().join(format!("wc_backup_2000010{i}-000000.zip")), "").unwrap());
    std::thread::sleep(std::time::Duration::from_millis(20));
    let archive = backup(home.path()).unwrap();
    let left = std::fs::read_dir(home.path())
        .unwrap()
        .filter(|e| e.as_ref().unwrap().file_name().to_str().unwrap().starts_with("wc_backup_"));
    assert_eq!(left.count(), 5);
    assert!(archive.exists());
}

#[test]
fn diagnostics_hold_the_settings_and_whichever_logs_exist() {
    let dir = tempfile::tempdir().unwrap();
    ["webcontrol.json", "webcontrol-rs.log", "log.txt", "unrelated.txt"]
        .iter()
        .for_each(|name| std::fs::write(dir.path().join(name), "x").unwrap());
    let archive = diagnostics(dir.path()).unwrap();
    assert_eq!(archive.parent().unwrap(), dir.path());
    assert_eq!(names(&archive), ["log.txt", "webcontrol-rs.log", "webcontrol.json"]);
    assert!(backup(&dir.path().join("missing")).is_err());
}

/// Ported from test_restore, with a backup Python's zipfolder wrote.
#[test]
fn a_python_backup_restores_its_files_and_settings() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/python_backup.zip");
    let settings = restore(&fixture, dir.path()).unwrap();
    assert_eq!(settings.text_or_empty("Maslow Settings", "COMport"), "/dev/ttyPYTHON");
    assert!(settings.value("Maslow Settings", "macro2_title").is_some(), "the defaults it lacks are added");
    assert_eq!(std::fs::read_to_string(dir.path().join("gcode/jobs/leg.nc")).unwrap(), "G21\nG0 X1 Y1\n");
    assert!(dir.path().join("boards/shelf.brd").is_file());
}

#[test]
fn a_restore_without_usable_settings_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let cases = [
        vec![("notes.txt", "")],
        vec![("webcontrol.json", "[]"), ("gcode/x.nc", "G0")],
        vec![("webcontrol.json", "{not json")],
    ];
    cases.iter().for_each(|entries| {
        let archive = dir.path().join("bad.zip");
        std::fs::write(&archive, zip_of(entries)).unwrap();
        let target = dir.path().join("target");
        assert!(restore(&archive, &target).is_err(), "{entries:?}");
        assert!(!target.exists());
    });
    let not_zip = dir.path().join("plain.zip");
    std::fs::write(&not_zip, "hello").unwrap();
    assert!(restore(&not_zip, dir.path()).is_err());
    assert!(restore(&dir.path().join("missing.zip"), dir.path()).is_err());
}

#[test]
fn restore_entries_that_would_leave_the_directory_are_skipped() {
    let base = tempfile::tempdir().unwrap();
    let archive = base.path().join("slip.zip");
    std::fs::write(
        &archive,
        zip_of(&[("webcontrol.json", "{}"), ("../evil.txt", "x"), ("gcode/", ""), ("a/b.nc", "G0")]),
    )
    .unwrap();
    let target = base.path().join("home/.WebControl");
    restore(&archive, &target).unwrap();
    assert!(!base.path().join("home/evil.txt").exists());
    assert!(target.join("a/b.nc").is_file());
    assert!(target.join("gcode").is_dir());
}

/// Python's zipfile reads a Rust backup, when a Python is on the path.
#[test]
fn python_reads_a_rust_backup() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".WebControl/gcode")).unwrap();
    std::fs::write(home.path().join(".WebControl/webcontrol.json"), "{}").unwrap();
    std::fs::write(home.path().join(".WebControl/gcode/a.nc"), "G0 X1\n".repeat(100)).unwrap();
    let archive = backup(home.path()).unwrap();
    let script = "import sys, zipfile\nz = zipfile.ZipFile(sys.argv[1])\nassert z.testzip() is None\n\
                  print(','.join(sorted(z.namelist())))";
    let Ok(output) = std::process::Command::new("python3").args(["-c", script]).arg(&archive).output() else {
        return eprintln!("no python3; skipped");
    };
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "gcode/a.nc,webcontrol.json");
}

#[tokio::test]
async fn the_backup_and_diagnostics_routes_answer_zips() {
    let server = Server::new();
    server.write("webcontrol.json", "{}");
    server.write("gcode/part.nc", "G0 X1\n");
    let backup = server.get("/backupWebControl?rnd=1").await;
    assert_eq!((backup.status, backup.content_type.as_str()), (StatusCode::OK, "application/zip"));
    assert!(backup.disposition.as_deref().unwrap().starts_with("inline; filename=wc_backup_"));
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(backup.body)).unwrap();
    let mut text = String::new();
    zip.by_name("gcode/part.nc").unwrap().read_to_string(&mut text).unwrap();
    assert_eq!(text, "G0 X1\n");
    let diagnostics = server.get("/downloadDiagnostics?rnd=2").await;
    assert_eq!(diagnostics.status, StatusCode::OK);
    assert!(diagnostics.disposition.as_deref().unwrap().starts_with("attachment; filename=wc_diagnostics_"));
}

#[tokio::test]
async fn archives_that_cannot_be_written_fail() {
    let server = Server::new();
    std::fs::remove_dir_all(server.dir()).unwrap();
    std::fs::write(server.dir(), "").unwrap();
    let reply = server.get("/downloadDiagnostics").await;
    assert_eq!((reply.status, reply.json()), (StatusCode::INTERNAL_SERVER_ERROR, json!("failed")));
    let home = server.home.path().to_path_buf();
    let mut permissions = std::fs::metadata(&home).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o500);
    std::fs::set_permissions(&home, permissions.clone()).unwrap();
    let reply = server.get("/backupWebControl").await;
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o700);
    std::fs::set_permissions(&home, permissions).unwrap();
    assert_eq!(reply.status, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn a_restore_upload_replaces_the_files_and_settings() {
    let server = Server::new();
    let backup = zip_of(&[
        ("webcontrol.json", "{\"Maslow Settings\": [{\"key\": \"COMport\", \"value\": \"/dev/ttyRESTORED\"}]}"),
        ("gcode/restored.nc", "G0 X0\n"),
    ]);
    let reply = server.post_form("/importRestoreWebControl", &[Part::File("file", "wc_backup.zip", &backup)]).await;
    assert_eq!((reply.status, reply.json()), (StatusCode::OK, json!({"status": 200})));
    assert!(server.home.path().join("wc_backup.zip").is_file(), "Python keeps the upload in the home");
    assert!(server.dir().join("gcode/restored.nc").is_file());
    server.settle().await;
    let snapshot = server.machine.snapshot();
    assert_eq!(snapshot.settings.text_or_empty("Maslow Settings", "COMport"), "/dev/ttyRESTORED");
    let saved: Value = server.saved_settings();
    assert!(saved["Maslow Settings"].to_string().contains("ttyRESTORED"));
    let bad = zip_of(&[("notes.txt", "")]);
    let reply = server.post_form("/importRestoreWebControl", &[Part::File("file", "bad.zip", &bad)]).await;
    assert_eq!(reply.json(), json!({"status": 500}));
    assert_eq!(server.post_unchecked("/importRestoreWebControl", &[]).await.status, StatusCode::BAD_REQUEST);
    let nameless = server.post_form("/importRestoreWebControl", &[Part::File("file", "", &backup)]).await;
    assert_eq!(nameless.status, StatusCode::INTERNAL_SERVER_ERROR, "Python cannot save a nameless upload");
}

#[tokio::test]
async fn settings_imports_change_the_settings() {
    let server = Server::new();
    let ini = b"[Maslow Settings]\ncomport = /dev/ttyINI\n";
    let reply = server.post_form("/importFile", &[Part::File("file", "groundcontrol.ini", ini)]).await;
    assert_eq!(reply.json(), json!({"status": 200}));
    assert!(server.dir().join("imports/groundcontrol.ini").is_file());
    assert_eq!(server.machine.snapshot().settings.text_or_empty("Maslow Settings", "COMport"), "/dev/ttyINI");
    let json = br#"{"Maslow Settings": [{"key": "COMport", "value": "/dev/ttyJSON"}]}"#;
    let reply = server.post_form("/importFileWCJSON", &[Part::File("file", "webcontrol.json", json)]).await;
    assert_eq!(reply.json(), json!({"status": 200}));
    assert_eq!(server.machine.snapshot().settings.text_or_empty("Maslow Settings", "COMport"), "/dev/ttyJSON");
    let mut pushes = server.machine.subscribe();
    let reply = server.post_form("/importFileWCJSON", &[Part::File("file", "bad.json", b"{nope")]).await;
    assert_eq!(reply.json(), json!({"status": 500}));
    let reply = server.post_form("/importFile", &[Part::File("file", "bad.ini", b"no section = 1\n")]).await;
    assert_eq!(reply.json(), json!({"status": 500}));
    server.settle().await;
    let notices = std::iter::from_fn(|| pushes.try_recv().ok()).filter(|p| p.data["command"] == "activateModal");
    assert_eq!(notices.count(), 2);
    assert_eq!(server.post_unchecked("/importFile", &[]).await.status, StatusCode::BAD_REQUEST);
}
