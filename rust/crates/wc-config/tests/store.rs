//! webcontrol.json's format (test_config_storage), a restored file (test_restore) and the settings imports.

use serde_json::{Value, json};
use wc_config::{Imported, Settings, Taken, python_json};

/// `json.dumps(value, sort_keys=True, indent=4, ensure_ascii=False)`, from Python 3.9.
const PYTHON_DUMP: &str = "{\n    \"a\": [],\n    \"b\": [\n        1,\n        2.5,\n        1e+16,\n        1e-05,\n        {\n            \"x\": \"é\u{2028}\\\"\\\\\\u0001\"\n        }\n    ],\n    \"c\": {},\n    \"d\": null,\n    \"e\": true,\n    \"f\": -0.0,\n    \"g\": 100.0\n}";

#[test]
fn the_file_is_written_as_pythons_json_dump_writes_it() {
    let value = json!({"b": [1, 2.5, 1e16, 1e-5, {"x": "é\u{2028}\"\\\u{1}"}], "a": [], "c": {}, "d": null,
        "e": true, "f": -0.0, "g": 100.0});
    assert_eq!(python_json(&value), PYTHON_DUMP);
}

#[test]
fn restored_text_must_be_an_object_and_gains_the_missing_defaults() {
    let restored = Settings::from_text("{\"Maslow Settings\": [{\"key\": \"COMport\", \"value\": \"/dev/ttyX\"}]}");
    let restored = restored.unwrap();
    assert_eq!(restored.text("Maslow Settings", "COMport").as_deref(), Some("/dev/ttyX"));
    assert!(restored.value("Maslow Settings", "macro2_title").is_some());
    ["[]", "\"settings\"", "{not json"].iter().for_each(|text| assert!(Settings::from_text(text).is_none(), "{text}"));
}

fn refused(taken: Taken) -> bool {
    matches!(taken, Taken::Refused(_))
}

#[test]
fn an_import_converts_text_to_each_options_type() {
    let mut settings = Settings::defaults();
    assert_eq!(settings.set_typed("Maslow Settings", "sledweight", " 97.5 "), Taken::Stored);
    assert_eq!(settings.value("Maslow Settings", "sledWeight"), Some(&json!(97.5)));
    assert_eq!(settings.set_typed("Advanced Settings", "truncate", "1"), Taken::Stored);
    assert_eq!(settings.value("Advanced Settings", "truncate"), Some(&json!(1)));
    assert_eq!(settings.set_typed("Maslow Settings", "COMport", "/dev/ttyUSB0"), Taken::Stored);
    assert_eq!(settings.value("Maslow Settings", "COMport"), Some(&json!("/dev/ttyUSB0")));
    assert_eq!(settings.set_typed("Optical Calibration Settings", "gaussianBlurValue", "7"), Taken::Stored);
    assert_eq!(settings.value("Optical Calibration Settings", "gaussianBlurValue"), Some(&json!(7)));
    let fraction = settings.set_typed("Optical Calibration Settings", "gaussianBlurValue", "7.5");
    assert_eq!(fraction.note().as_deref(), Some("Kept gaussianBlurValue as it was: \"7.5\" is not a whole number."));
    assert!(refused(settings.set_typed("Maslow Settings", "sledWeight", "heavy")));
    assert_eq!(settings.value("Maslow Settings", "sledWeight"), Some(&json!(97.5)));
    assert_eq!(settings.set_typed("Maslow Settings", "noSuchKey", "1"), Taken::Unknown);
    assert_eq!(settings.set_typed("No Section", "bedWidth", "1"), Taken::Unknown);
}

/// Upstream #90: GroundControl writes `zAxis = 1.0`, which Python stored as off. A deliberate deviation.
#[test]
fn an_imported_bool_is_on_for_any_non_zero_number_and_the_usual_words() {
    let mut settings = Settings::defaults();
    let set = |settings: &mut Settings, text: &str| {
        settings.set_typed("Advanced Settings", "truncate", "0");
        let taken = settings.set_typed("Maslow Settings", "zAxis", text);
        (taken, settings.value("Maslow Settings", "zAxis").cloned())
    };
    ["1.0", "1", "2", "-1", "true", "On", "YES"]
        .iter()
        .for_each(|text| assert_eq!(set(&mut settings, text), (Taken::Stored, Some(json!(1))), "{text}"));
    ["0", "0.0", "-0", "off", "False", "no"]
        .iter()
        .for_each(|text| assert_eq!(set(&mut settings, text), (Taken::Stored, Some(json!(0))), "{text}"));
    settings.set_typed("Maslow Settings", "zAxis", "1");
    let (taken, value) = set(&mut settings, "maybe");
    assert_eq!(taken.note().as_deref(), Some("Kept zAxis as it was: \"maybe\" is neither on nor off."));
    assert_eq!(value, Some(json!(1)));
    assert!(refused(settings.set_typed("Maslow Settings", "zAxis", "nan")));
}

/// Upstream #135: a decimal comma in a numeric setting broke Home and Return to Center.
#[test]
fn a_decimal_comma_in_a_numeric_setting_becomes_a_point_or_is_refused() {
    let mut settings = Settings::defaults();
    let converted = settings.set_typed("Maslow Settings", "zAxisSafeHeight", "0,01");
    assert_eq!(converted, Taken::Converted("Read zAxisSafeHeight \"0,01\" as 0.01.".into()));
    assert_eq!(settings.value("Maslow Settings", "zAxisSafeHeight"), Some(&json!("0.01")));
    assert!(matches!(settings.set_typed("Maslow Settings", "sledWeight", "97,5"), Taken::Converted(_)));
    assert_eq!(settings.value("Maslow Settings", "sledWeight"), Some(&json!(97.5)));
    settings.set_typed("Maslow Settings", "bedWidth", "2438,4");
    assert_eq!(settings.value("Maslow Settings", "bedWidth"), Some(&json!("2438.4")));
    let grouped = settings.set_typed("Maslow Settings", "zAxisSafeHeight", "1,250");
    let note = "Kept zAxisSafeHeight as it was: \"1,250\" could be 1.250 or 1250; write it with a point.";
    assert_eq!(grouped.note().as_deref(), Some(note));
    ["1,2,3", "1.5,2", "abc", ""].iter().for_each(|text| {
        assert!(refused(settings.set_typed("Maslow Settings", "zAxisSafeHeight", text)), "{text}");
    });
    assert_eq!(settings.value("Maslow Settings", "zAxisSafeHeight"), Some(&json!("0.01")));
    assert_eq!(settings.set_typed("Maslow Settings", "zAxisSafeHeight", "7"), Taken::Stored);
    assert_eq!(settings.set_typed("Advanced Settings", "maxZRPM", ""), Taken::Stored, "blank until read");
    assert_eq!(settings.set_typed("Maslow Settings", "macro1_title", "1,250"), Taken::Stored, "not a number");
    assert_eq!(settings.set_typed("Advanced Settings", "fPWM", "31,000Hz"), Taken::Stored);
    assert_eq!(settings.set_typed("Optical Calibration Settings", "xyErrorArray", "0,0"), Taken::Stored);
}

#[test]
fn an_ini_import_reports_what_it_converted_or_kept() {
    let mut settings = Settings::defaults();
    let ini = "[Maslow Settings]\nzAxis = 1.0\nzAxisSafeHeight = 0,01\nsledWeight = heavy\nnoSuchKey = 1\n";
    let imported = settings.import_ini(ini).unwrap();
    assert_eq!(
        imported,
        Imported {
            changed: true,
            notes: vec![
                "Read zAxisSafeHeight \"0,01\" as 0.01.".into(),
                "Kept sledWeight as it was: \"heavy\" is not a number.".into(),
            ],
        }
    );
    assert_eq!(settings.value("Maslow Settings", "zAxis"), Some(&json!(1)));
}

#[test]
fn a_webcontrol_json_import_checks_numeric_text() {
    let mut settings = Settings::defaults();
    let imported = json!({"Maslow Settings": [
        {"key": "zAxisSafeHeight", "value": "0,01"},
        {"key": "bedWidth", "value": "1,250"},
        {"key": "sledWeight", "value": "heavy"},
        {"key": "macro1_title", "value": "1,250"},
        {"key": "motorOffsetY", "value": "500"},
        {"key": "maxZRPM", "value": ""}
    ]});
    let imported = settings.import_webcontrol(&imported).unwrap();
    assert!(imported.changed);
    assert_eq!(
        imported.notes,
        [
            "Read zAxisSafeHeight \"0,01\" as 0.01.",
            "Kept bedWidth as it was: \"1,250\" could be 1.250 or 1250; write it with a point.",
            "Kept sledWeight as it was: \"heavy\" is not a number.",
        ]
    );
    assert_eq!(settings.value("Maslow Settings", "zAxisSafeHeight"), Some(&json!("0.01")));
    assert_eq!(settings.value("Maslow Settings", "bedWidth"), Some(&json!("2438.4")));
    assert_eq!(settings.value("Maslow Settings", "macro1_title"), Some(&json!("1,250")));
    assert_eq!(settings.value("Maslow Settings", "motorOffsetY"), Some(&json!("500")));
}

#[test]
fn a_groundcontrol_ini_sets_the_sections_webcontrol_knows() {
    let mut settings = Settings::defaults();
    let ini = "[Maslow Settings]\r\nsledWeight = 2000\r\n\r\n[Ground Control Settings]\r\nwhatever\r\n\
               [Advanced Settings]\r\nhomeX = 12.5\r\n";
    assert_eq!(settings.import_ini(ini), Some(Imported { changed: true, notes: vec![] }));
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
    assert!(settings.import_webcontrol(&imported).unwrap().changed);
    assert_eq!(settings.value("Maslow Settings", "COMport"), Some(&json!("/dev/ttyIMPORTED")));
    let added = settings.option("Advanced Settings", "brandNew").unwrap();
    assert_eq!(Value::Object(added.clone()), json!({"key": "brandNew", "value": 3, "title": "New"}));
    assert_eq!(settings.text("Mine", "mine").as_deref(), Some("x"));
    assert_eq!(settings.import_webcontrol(&imported), Some(Imported::default()), "nothing new the second time");
    [json!([]), json!({"Maslow Settings": {}}), json!({"Maslow Settings": [1]}), json!({"M": [{"value": 1}]})]
        .iter()
        .for_each(|bad| assert_eq!(Settings::defaults().import_webcontrol(bad), None, "{bad}"));
    let mut odd = Settings::from_text("{\"Odd\": 5}").unwrap();
    assert_eq!(odd.import_webcontrol(&json!({"Odd": [{"key": "k", "value": 1}]})), Some(Imported::default()));
}
