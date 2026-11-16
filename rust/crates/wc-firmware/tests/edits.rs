//! The `$` lines a settings edit sends (`Config.setValue` → `syncFirmwareKey`), checked against what Python sent
//! for the same edits (fixtures/rust_settings/edit-lines.json).

use std::path::PathBuf;

use serde_json::{Value, json};
use wc_config::{Paths, Settings};
use wc_firmware::settings::{setting_key, setting_line, write};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures/rust_settings")
}

fn python_written() -> Settings {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths::new(home.path());
    std::fs::create_dir_all(paths.dir()).unwrap();
    std::fs::copy(fixtures().join("python-written.json"), paths.settings_file()).unwrap();
    Settings::load(&paths)
}

#[test]
fn edits_send_the_lines_python_sends() {
    let mut settings = python_written();
    let edits = [
        ("Maslow Settings", "motorSpacingX", json!("3000")),
        ("Maslow Settings", "zAxis", json!(true)),
        ("Maslow Settings", "macro2_title", json!("Vide \u{2013} poussière")),
        ("WebControl Settings", "webPort", json!("5001")),
        ("WebControl Settings", "loggingTimeout", json!("2.5")),
        ("Advanced Settings", "chainOverSprocket", json!("Top")),
        ("Advanced Settings", "enablePosPIDValues", json!(true)),
        ("Advanced Settings", "KpPos", json!("1400")),
        ("Maslow Settings", "sledWeight", json!("101.25")),
    ];
    let sent: Vec<Vec<String>> =
        edits.iter().map(|(section, key, value)| write(&mut settings, 0.0, section, key, value).unwrap()).collect();
    let python: Vec<Vec<String>> =
        serde_json::from_str(&std::fs::read_to_string(fixtures().join("edit-lines.json")).unwrap()).unwrap();
    // The same values, each as the text the firmware reads exactly (`spec/firmware.md` §4).
    let exact = |line: &String| {
        let (_, text) = line.split_once('=').unwrap();
        setting_line(setting_key(line).unwrap(), &Value::String(text.into()))
    };
    let expected: Vec<Vec<String>> = python.iter().map(|lines| lines.iter().map(exact).collect()).collect();
    assert_eq!(sent, expected);
    assert_eq!(sent[7..], [vec!["$21=1400".to_string()], vec!["$46=101.24999".to_string()]]);
}

fn lines(settings: &mut Settings, firmware: f64, section: &str, key: &str, value: Value) -> Vec<String> {
    write(settings, firmware, section, key, &value).unwrap()
}

#[test]
fn an_unchanged_value_sends_nothing_and_a_blank_one_is_left_for_the_controller() {
    let mut settings = Settings::defaults();
    assert!(lines(&mut settings, 51.27, "Maslow Settings", "bedWidth", json!("2438.40")).is_empty());
    assert!(lines(&mut settings, 51.27, "Advanced Settings", "maxZRPM", json!("")).is_empty());
    assert_eq!(lines(&mut settings, 51.27, "Advanced Settings", "maxZRPM", json!("12.6")), ["$18=12.6"]);
    assert!(lines(&mut settings, 51.27, "Advanced Settings", "maxZRPM", json!("")).is_empty());
}

#[test]
fn the_spindle_mode_goes_out_as_its_code() {
    let mut settings = Settings::defaults();
    assert_eq!(lines(&mut settings, 51.27, "Advanced Settings", "spindleAutomate", json!("Servo")), ["$17=1"]);
    assert_eq!(lines(&mut settings, 51.27, "Advanced Settings", "spindleAutomate", json!("Relay_Low")), ["$17=3"]);
    assert!(lines(&mut settings, 51.27, "Advanced Settings", "spindleAutomate", json!("Relay_Low")).is_empty());
}

#[test]
fn derived_settings_follow_with_their_own_lines() {
    let mut settings = Settings::defaults();
    assert_eq!(
        lines(&mut settings, 51.27, "Advanced Settings", "gearTeeth", json!("9")),
        ["$13=57.150"],
        "57.15 reads as 57.1500053"
    );
    assert_eq!(lines(&mut settings, 51.27, "Advanced Settings", "kinematicsType", json!("Quadrilateral")), ["$7=1"]);
    assert_eq!(lines(&mut settings, 51.27, "Advanced Settings", "fPWM", json!("4,100Hz")), ["$39=2"]);
}

/// The owner's controller on 2026-11-15: `$24=1.000000` read as 1.00000012, above the 1 `SetTunings` accepts.
#[test]
fn a_pid_weight_change_is_written_as_text_the_firmware_reads_exactly() {
    let mut settings = Settings::defaults();
    assert!(lines(&mut settings, 51.27, "Advanced Settings", "enablePosPIDValues", json!(true)).is_empty());
    assert_eq!(lines(&mut settings, 51.27, "Advanced Settings", "propWeight", json!("0.5")), ["$24=0.5"]);
    assert_eq!(lines(&mut settings, 51.27, "Advanced Settings", "propWeight", json!("1")), ["$24=1"]);
    assert_eq!(
        lines(&mut settings, 51.27, "Advanced Settings", "KdV", json!("0.29")),
        ["$27=0.28999994"],
        "0.29 and 0.290 read high"
    );
}

#[test]
fn text_the_controller_cannot_read_is_never_sent() {
    let mut settings = Settings::defaults();
    assert!(lines(&mut settings, 51.27, "Maslow Settings", "bedWidth", json!("wide")).is_empty());
    assert!(lines(&mut settings, 51.27, "Maslow Settings", "bedWidth", json!("2000")).is_empty());
}

#[test]
fn optical_and_unsendable_keys_need_custom_firmware_or_never_go() {
    let mut settings = Settings::defaults();
    assert!(lines(&mut settings, 51.27, "Optical Calibration Settings", "calX0", json!("0.5")).is_empty());
    assert_eq!(lines(&mut settings, 101.0, "Optical Calibration Settings", "calX0", json!("0.6")), ["$87=0.6"]);
    assert!(lines(&mut settings, 101.0, "Optical Calibration Settings", "calX0", json!("0.60001")).is_empty());
    assert!(lines(&mut settings, 101.0, "Advanced Settings", "enableOpticalCalibration", json!(true)).is_empty());
    assert!(lines(&mut settings, 101.0, "Optical Calibration Settings", "xyErrorArray", json!("1,2")).is_empty());
}

#[test]
fn a_custom_setting_waits_for_firmware_over_100() {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths::new(home.path());
    std::fs::create_dir_all(paths.dir()).unwrap();
    let file = json!({"Maslow Settings": [{"key": "bedWidth", "value": "1", "type": "string", "firmwareKey": 0, "custom": 1}]});
    std::fs::write(paths.settings_file(), file.to_string()).unwrap();
    let mut custom = Settings::load(&paths);
    assert!(lines(&mut custom, 100.0, "Maslow Settings", "bedWidth", json!("2")).is_empty());
    assert_eq!(lines(&mut custom, 100.5, "Maslow Settings", "bedWidth", json!("3")), ["$0=3"]);
}

#[test]
fn nothing_is_sent_for_a_value_that_is_not_stored() {
    let mut settings = Settings::defaults();
    assert_eq!(write(&mut settings, 51.27, "Maslow Settings", "nope", &json!(1)), None);
}
