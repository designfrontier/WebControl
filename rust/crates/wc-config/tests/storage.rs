//! Writing the shared `webcontrol.json`; the first half is ported from tests/test_config_storage.py.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use serde_json::{Value, json};
use wc_config::{Paths, Settings, Source, python_json};

fn read(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn port_of(settings: &Value) -> Value {
    let maslow = settings["Maslow Settings"].as_array().unwrap();
    maslow.iter().find(|o| o["key"] == "COMport").unwrap()["value"].clone()
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> =
        fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

fn saved_with_port(paths: &Paths, settings: &mut Settings, port: &str) {
    settings.set_value("Maslow Settings", "COMport", json!(port));
    settings.save(paths).unwrap();
}

#[test]
fn saving_keeps_the_previous_file_as_a_backup() {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths::new(home.path());
    let mut settings = Settings::open(&paths).unwrap();
    saved_with_port(&paths, &mut settings, "/dev/ttyACM0");
    saved_with_port(&paths, &mut settings, "/dev/ttyACM1");
    assert_eq!(port_of(&read(&paths.settings_file())), "/dev/ttyACM1");
    assert_eq!(port_of(&read(&paths.settings_backup())), "/dev/ttyACM0");
    assert!(!names(&paths.dir()).iter().any(|n| n.ends_with(".tmp")));
}

#[test]
fn a_failed_write_leaves_the_settings_file_intact() {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths::new(home.path());
    let mut settings = Settings::open(&paths).unwrap();
    saved_with_port(&paths, &mut settings, "/dev/ttyACM0");
    let before = fs::read(paths.settings_file()).unwrap();
    fs::set_permissions(paths.dir(), fs::Permissions::from_mode(0o500)).unwrap();
    settings.set_value("Maslow Settings", "COMport", json!("/dev/ttyACM1"));
    let failed = settings.save(&paths);
    fs::set_permissions(paths.dir(), fs::Permissions::from_mode(0o700)).unwrap();
    assert!(failed.is_err());
    assert_eq!(fs::read(paths.settings_file()).unwrap(), before);
    assert!(!names(&paths.dir()).iter().any(|n| n.ends_with(".tmp")));
}

#[test]
fn a_corrupt_settings_file_falls_back_to_the_backup_and_is_set_aside() {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths::new(home.path());
    let mut settings = Settings::open(&paths).unwrap();
    saved_with_port(&paths, &mut settings, "/dev/ttyACM0");
    saved_with_port(&paths, &mut settings, "/dev/ttyACM1");
    fs::write(paths.settings_file(), r#"{"Maslow Settings": ["#).unwrap();
    let reopened = Settings::open(&paths).unwrap();
    assert_eq!(reopened.source(), Source::Backup);
    assert_eq!(reopened.text("Maslow Settings", "COMport").as_deref(), Some("/dev/ttyACM0"));
    assert_eq!(port_of(&read(&paths.settings_file())), "/dev/ttyACM0");
    let corrupt = paths.dir().join("webcontrol.json.corrupt");
    assert_eq!(fs::read_to_string(corrupt).unwrap(), r#"{"Maslow Settings": ["#);
}

#[test]
fn corrupt_settings_and_backup_fall_back_to_the_defaults() {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths::new(home.path());
    Settings::open(&paths).unwrap();
    fs::write(paths.settings_file(), "").unwrap();
    fs::write(paths.settings_backup(), "not json").unwrap();
    let reopened = Settings::open(&paths).unwrap();
    assert_eq!(reopened.source(), Source::Defaults);
    assert_eq!(reopened.text("Maslow Settings", "COMport").as_deref(), Some(""));
    assert!(read(&paths.settings_file())["Maslow Settings"].is_array());
}

#[test]
fn a_missing_settings_file_with_a_backup_restores_the_backup() {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths::new(home.path());
    let mut settings = Settings::open(&paths).unwrap();
    saved_with_port(&paths, &mut settings, "/dev/ttyACM0");
    saved_with_port(&paths, &mut settings, "/dev/ttyACM1");
    fs::remove_file(paths.settings_file()).unwrap();
    let reopened = Settings::open(&paths).unwrap();
    assert_eq!(reopened.text("Maslow Settings", "COMport").as_deref(), Some("/dev/ttyACM0"));
    assert_eq!(port_of(&read(&paths.settings_file())), "/dev/ttyACM0");
}

#[test]
fn a_first_run_copies_the_defaults_verbatim_and_makes_the_directories() {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths::new(home.path());
    let settings = Settings::open(&paths).unwrap();
    assert_eq!(settings.source(), Source::File);
    let defaults = include_str!("../../../../defaultwebcontrol.json");
    assert_eq!(fs::read_to_string(paths.settings_file()).unwrap(), defaults);
    assert_eq!(names(&paths.dir()), ["boards", "gcode", "imports", "webcontrol.json", "webcontrol.json.pre-rs"]);
}

#[test]
fn the_file_python_wrote_is_kept_once_as_pre_rs() {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths::new(home.path());
    fs::create_dir_all(paths.dir()).unwrap();
    fs::write(paths.settings_file(), r#"{"Mine": [{"key": "x", "value": 1}]}"#).unwrap();
    let mut settings = Settings::open(&paths).unwrap();
    assert_eq!(fs::read_to_string(paths.settings_snapshot()).unwrap(), r#"{"Mine": [{"key": "x", "value": 1}]}"#);
    assert_eq!(read(&paths.settings_file())["Mine"], json!([{"key": "x", "value": 1}]));
    saved_with_port(&paths, &mut settings, "/dev/ttyACM9");
    Settings::open(&paths).unwrap();
    assert_eq!(fs::read_to_string(paths.settings_snapshot()).unwrap(), r#"{"Mine": [{"key": "x", "value": 1}]}"#);
}

#[test]
fn a_complete_file_is_not_rewritten_on_open() {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths::new(home.path());
    Settings::open(&paths).unwrap();
    let before = fs::read(paths.settings_file()).unwrap();
    Settings::open(&paths).unwrap();
    assert_eq!(fs::read(paths.settings_file()).unwrap(), before);
    assert!(!paths.settings_backup().exists());
}

#[test]
fn opening_fails_where_the_home_cannot_be_written() {
    let home = tempfile::tempdir().unwrap();
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o500)).unwrap();
    let opened = Settings::open(&Paths::new(home.path()));
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700)).unwrap();
    assert!(opened.is_err());
}

#[test]
fn the_format_is_pythons_json_dump_with_sorted_keys_and_four_spaces() {
    let value =
        json!({"b": [1, 2.5, 5.1685e-06, 1e16, "Ünïcode \"q\"\n", true, null, [], {}], "a": {"z": 0.1, "y": -0.0}});
    let expected = "{\n    \"a\": {\n        \"y\": -0.0,\n        \"z\": 0.1\n    },\n    \"b\": [\n        1,\n        2.5,\n        \
                    5.1685e-06,\n        1e+16,\n        \"Ünïcode \\\"q\\\"\\n\",\n        true,\n        null,\n        \
                    [],\n        {}\n    ]\n}";
    assert_eq!(python_json(&value), expected);
    assert_eq!(python_json(&json!("\u{1}")), "\"\\u0001\"");
}

#[test]
fn saving_writes_every_section_and_unknown_field_back() {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths::new(home.path());
    fs::create_dir_all(paths.dir()).unwrap();
    let file = json!({"Mine": [{"key": "x", "value": 1, "extra": {"deep": [1.5]}}], "Odd": "text"});
    fs::write(paths.settings_file(), file.to_string()).unwrap();
    Settings::open(&paths).unwrap();
    let saved = read(&paths.settings_file());
    assert_eq!(saved["Mine"], file["Mine"]);
    assert_eq!(saved["Odd"], "text");
    assert_eq!(saved["Maslow Settings"].as_array().unwrap().len(), 19);
}
