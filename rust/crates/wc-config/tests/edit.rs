//! `Config.setValue`'s conversions and `Config.computeSettings`.

use serde_json::{Value, json};
use wc_config::{Settings, Stored, python_eq, python_float};

fn stored_value(settings: &Settings, section: &str, key: &str) -> Value {
    settings.value(section, key).cloned().unwrap()
}

#[test]
fn values_are_converted_by_the_settings_type() {
    let mut settings = Settings::defaults();
    let cases = [
        ("WebControl Settings", "webPort", json!("5001"), json!(5001)),
        ("WebControl Settings", "webPort", json!(5002.7), json!(5002)),
        ("WebControl Settings", "loggingTimeout", json!(" 2.5 "), json!(2.5)),
        ("WebControl Settings", "loggingTimeout", json!(true), json!(1.0)),
        ("Maslow Settings", "zAxis", json!(true), json!(1)),
        ("Maslow Settings", "zAxis", json!("off"), json!(0)),
        ("Maslow Settings", "zAxis", json!("on"), json!(1)),
        ("Maslow Settings", "zAxis", json!(0), json!(0)),
        ("Maslow Settings", "zAxis", json!("1"), json!(1)),
        ("Maslow Settings", "zAxis", json!(false), json!(0)),
        ("Maslow Settings", "macro1", json!("G0 X0"), json!("G0 X0")),
        ("Maslow Settings", "macro1", json!(12), json!(12)),
        ("Advanced Settings", "chainOverSprocket", json!("Top"), json!("Top")),
    ];
    cases.iter().for_each(|(section, key, value, expected)| {
        settings.store(section, key, value).unwrap();
        assert_eq!(&stored_value(&settings, section, key), expected, "{key} = {value}");
    });
}

#[test]
fn a_value_python_cannot_convert_is_not_stored() {
    let mut settings = Settings::defaults();
    [("WebControl Settings", "webPort", json!("50.5")), ("WebControl Settings", "loggingTimeout", json!("fast"))]
        .iter()
        .for_each(|(section, key, value)| {
            let before = stored_value(&settings, section, key);
            assert_eq!(settings.store(section, key, value), None, "{key}");
            assert_eq!(stored_value(&settings, section, key), before);
        });
    assert_eq!(settings.store("Maslow Settings", "nope", &json!(1)), None);
    assert_eq!(settings.store("Nope", "bedWidth", &json!(1)), None);
    assert_eq!(settings.store("WebControl Settings", "loggingTimeout", &json!(null)), None);
}

#[test]
fn a_store_reports_what_it_replaced_and_the_controller_key() {
    let mut settings = Settings::defaults();
    let stored = settings.store("Maslow Settings", "motorspacingx", &json!("3000")).unwrap();
    assert_eq!(
        stored,
        Stored {
            section: "Maslow Settings".into(),
            key: "motorSpacingX".into(),
            previous: json!("2978.4"),
            firmware_key: Some(2)
        }
    );
    let untyped = settings.store("Maslow Settings", "COMport", &json!("/dev/ttyACM0")).unwrap();
    assert_eq!(untyped.firmware_key, None);
}

#[test]
fn computing_from_the_defaults_changes_nothing() {
    let mut settings = Settings::defaults();
    assert_eq!(settings.compute(), Vec::new());
}

fn computed(settings: &mut Settings) -> Vec<(String, Value)> {
    settings
        .compute()
        .into_iter()
        .map(|stored| (stored.key.clone(), stored_value(settings, &stored.section, &stored.key)))
        .collect()
}

#[test]
fn computed_settings_follow_the_advanced_ones_in_pythons_order() {
    let mut settings = Settings::defaults();
    [
        ("kinematicsType", json!("Quadrilateral")),
        ("gearTeeth", json!("9")),
        ("enablePosPIDValues", json!(1)),
        ("KpPos", json!("1400")),
        ("KdV", json!("0.5")),
        ("chainOverSprocket", json!("Top")),
        ("fPWM", json!("31,000Hz")),
    ]
    .iter()
    .for_each(|(key, value)| assert!(settings.set_value("Advanced Settings", key, value.clone())));
    let changes = computed(&mut settings);
    assert_eq!(
        changes,
        vec![
            ("kinematicsTypeComputed".into(), json!("1")),
            ("distPerRot".into(), json!("57.15")),
            ("KpPosMain".into(), json!(1400.0)),
            ("KdVMain".into(), json!(0.5)),
            ("chainOverSprocketComputed".into(), json!(1)),
            ("fPWMComputed".into(), json!(1)),
        ]
    );
    assert_eq!(settings.compute(), Vec::new());
}

/// Python decides the velocity gains by enablePosPIDValues too; enableVPIDValues does nothing.
#[test]
fn the_velocity_gains_follow_the_position_switch_as_in_python() {
    let mut settings = Settings::defaults();
    settings.set_value("Advanced Settings", "KpV", json!("7"));
    settings.set_value("Advanced Settings", "enableVPIDValues", json!(1));
    assert_eq!(computed(&mut settings), Vec::new());
    settings.set_value("Advanced Settings", "enablePosPIDValues", json!(1));
    assert_eq!(computed(&mut settings), vec![("KpVMain".to_string(), json!(7.0))]);
    settings.set_value("Advanced Settings", "enablePosPIDValues", json!(0));
    settings.set_value("Advanced Settings", "fPWM", json!("4,100Hz"));
    settings.set_value("Advanced Settings", "chainOverSprocket", json!("Sideways"));
    assert_eq!(
        computed(&mut settings),
        vec![("KpVMain".to_string(), json!(5)), ("fPWMComputed".to_string(), json!(2))]
    );
}

#[test]
fn unreadable_inputs_leave_their_computed_setting_alone() {
    let mut settings = Settings::defaults();
    settings.set_value("Advanced Settings", "chainPitch", json!("pitch"));
    settings.set_value("Advanced Settings", "enablePosPIDValues", json!(1));
    settings.set_value("Advanced Settings", "KiPos", json!("x"));
    let changes = computed(&mut settings);
    assert!(changes.iter().all(|(key, _)| key != "distPerRot" && key != "KiPosMain"), "{changes:?}");
}

#[test]
fn python_equality_and_float_read_numbers_and_flags_alike() {
    assert!(python_eq(&json!(1300), &json!(1300.0)));
    assert!(python_eq(&json!(true), &json!(1)));
    assert!(!python_eq(&json!(1), &json!("1")));
    assert!(python_eq(&json!("a"), &json!("a")));
    assert_eq!(python_float(&json!(" 1.5 ")), Some(1.5));
    assert_eq!(python_float(&json!("nan")), None);
    assert_eq!(python_float(&json!(false)), Some(0.0));
    assert_eq!(python_float(&json!([1])), None);
}
