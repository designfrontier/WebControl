//! Writing webcontrol.json as Python does (test_config_storage, test_restore) and the settings imports.

use std::fs;

use serde_json::{Value, json};
use wc_config::{Paths, Settings, python_json};

/// `json.dumps(value, sort_keys=True, indent=4, ensure_ascii=False)`, from Python 3.9.
const PYTHON_DUMP: &str = "{\n    \"a\": [],\n    \"b\": [\n        1,\n        2.5,\n        1e+16,\n        1e-05,\n        {\n            \"x\": \"é\u{2028}\\\"\\\\\\u0001\"\n        }\n    ],\n    \"c\": {},\n    \"d\": null,\n    \"e\": true,\n    \"f\": -0.0,\n    \"g\": 100.0\n}";

#[test]
fn the_file_is_written_as_pythons_json_dump_writes_it() {
    let value = json!({"b": [1, 2.5, 1e16, 1e-5, {"x": "é\u{2028}\"\\\u{1}"}], "a": [], "c": {}, "d": null,
        "e": true, "f": -0.0, "g": 100.0});
    assert_eq!(python_json(&value), PYTHON_DUMP);
}

#[test]
fn saving_keeps_the_previous_file_as_bak_and_the_python_file_as_pre_rs() {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths::new(home.path());
    let file = paths.settings_file();
    fs::create_dir_all(paths.dir()).unwrap();
    fs::write(&file, "{\"Maslow Settings\": []}").unwrap();
    let mut settings = Settings::load(&paths);
    settings.set_value("Maslow Settings", "COMport", json!("/dev/ttyACM0"));
    settings.save_to(&file).unwrap();
    let python = "{\"Maslow Settings\": []}";
    assert_eq!(fs::read_to_string(file.with_extension("json.pre-rs")).unwrap(), python);
    assert_eq!(fs::read_to_string(file.with_extension("json.bak")).unwrap(), python);
    let reloaded = Settings::load(&paths);
    assert_eq!(reloaded.value("Maslow Settings", "COMport"), Some(&json!("/dev/ttyACM0")));
    settings.set_value("Maslow Settings", "COMport", json!("/dev/ttyUSB0"));
    settings.save_to(&file).unwrap();
    assert_eq!(fs::read_to_string(file.with_extension("json.pre-rs")).unwrap(), python, "taken only once");
    let bak: Value = serde_json::from_str(&fs::read_to_string(file.with_extension("json.bak")).unwrap()).unwrap();
    assert!(bak.to_string().contains("ttyACM0"));
    assert!(!file.with_extension("json.tmp").exists());
}

#[test]
fn a_first_save_creates_the_directory() {
    let home = tempfile::tempdir().unwrap();
    let file = Paths::new(home.path()).settings_file();
    Settings::defaults().save_to(&file).unwrap();
    assert!(file.is_file());
    assert!(!file.with_extension("json.pre-rs").exists());
    let blocked = home.path().join("blocker");
    fs::write(&blocked, "").unwrap();
    assert!(Settings::defaults().save_to(&blocked.join("webcontrol.json")).is_err());
}

#[test]
fn restored_text_must_be_an_object_and_gains_the_missing_defaults() {
    let restored = Settings::from_text("{\"Maslow Settings\": [{\"key\": \"COMport\", \"value\": \"/dev/ttyX\"}]}");
    let restored = restored.unwrap();
    assert_eq!(restored.text("Maslow Settings", "COMport").as_deref(), Some("/dev/ttyX"));
    assert!(restored.value("Maslow Settings", "macro2_title").is_some());
    ["[]", "\"settings\"", "{not json"].iter().for_each(|text| assert!(Settings::from_text(text).is_none(), "{text}"));
}

#[test]
fn an_import_converts_text_to_each_options_type() {
    let mut settings = Settings::defaults();
    assert!(settings.set_typed("Maslow Settings", "sledweight", " 97.5 "));
    assert_eq!(settings.value("Maslow Settings", "sledWeight"), Some(&json!(97.5)));
    assert!(settings.set_typed("Advanced Settings", "truncate", "1"));
    assert_eq!(settings.value("Advanced Settings", "truncate"), Some(&json!(1)));
    assert!(settings.set_typed("Advanced Settings", "truncate", "True"));
    assert_eq!(settings.value("Advanced Settings", "truncate"), Some(&json!(0)));
    assert!(settings.set_typed("Maslow Settings", "COMport", "/dev/ttyUSB0"));
    assert_eq!(settings.value("Maslow Settings", "COMport"), Some(&json!("/dev/ttyUSB0")));
    assert!(settings.set_typed("Optical Calibration Settings", "gaussianBlurValue", "7"));
    assert_eq!(settings.value("Optical Calibration Settings", "gaussianBlurValue"), Some(&json!(7)));
    assert!(!settings.set_typed("Optical Calibration Settings", "gaussianBlurValue", "7.5"), "int() refuses it");
    assert!(!settings.set_typed("Maslow Settings", "sledWeight", "heavy"));
    assert!(!settings.set_typed("Maslow Settings", "noSuchKey", "1"));
    assert!(!settings.set_typed("No Section", "bedWidth", "1"));
}

#[test]
fn a_groundcontrol_ini_sets_the_sections_webcontrol_knows() {
    let mut settings = Settings::defaults();
    let ini = "[Maslow Settings]\r\nsledWeight = 2000\r\n\r\n[Ground Control Settings]\r\nwhatever\r\n\
               [Advanced Settings]\r\nhomeX = 12.5\r\n";
    assert_eq!(settings.import_ini(ini), Some(()));
    assert_eq!(settings.value("Maslow Settings", "sledWeight"), Some(&json!(2000.0)));
    assert_eq!(settings.text("Advanced Settings", "homeX").as_deref(), Some("12.5"));
    assert_eq!(Settings::defaults().import_ini("bedWidth = 1\n"), None, "no section yet");
    assert_eq!(Settings::defaults().import_ini("[Maslow Settings]\nbedWidth\n"), None, "no value");
}

#[test]
fn a_webcontrol_json_import_updates_values_and_adds_unknown_options() {
    let mut settings = Settings::defaults();
    let imported = json!({
        "Maslow Settings": [{"key": "COMport", "value": "/dev/ttyIMPORTED"}, {"key": "bedWidth"}],
        "Advanced Settings": [{"key": "brandNew", "value": 3, "title": "New", "junk": true}],
        "Mine": [{"key": "mine", "value": "x"}]
    });
    assert_eq!(settings.import_webcontrol(&imported), Some(true));
    assert_eq!(settings.value("Maslow Settings", "COMport"), Some(&json!("/dev/ttyIMPORTED")));
    let added = settings.option("Advanced Settings", "brandNew").unwrap();
    assert_eq!(Value::Object(added.clone()), json!({"key": "brandNew", "value": 3, "title": "New"}));
    assert_eq!(settings.text("Mine", "mine").as_deref(), Some("x"));
    assert_eq!(settings.import_webcontrol(&imported), Some(false), "nothing new the second time");
    [json!([]), json!({"Maslow Settings": {}}), json!({"Maslow Settings": [1]}), json!({"M": [{"value": 1}]})]
        .iter()
        .for_each(|bad| assert_eq!(Settings::defaults().import_webcontrol(bad), None, "{bad}"));
    let mut odd = Settings::from_text("{\"Odd\": 5}").unwrap();
    assert_eq!(odd.import_webcontrol(&json!({"Odd": [{"key": "k", "value": 1}]})), Some(false));
}
