//! The settings screens' pure parts, ported from test_api_settings and test_api_machine.

use serde_json::{Value, json};
use wc_config::{Paths, Settings};
use wc_machine::forms::{
    errors, is_serial_port_name, machine_setup, pid_settings, quick_configure, section, section_title,
};

fn loaded(file: Value) -> Settings {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths::new(home.path());
    std::fs::create_dir_all(paths.dir()).unwrap();
    std::fs::write(paths.settings_file(), file.to_string()).unwrap();
    Settings::load(&paths)
}

fn with(changes: &[(&str, &str, Value)]) -> Settings {
    let mut settings = Settings::defaults();
    changes.iter().for_each(|(section, key, value)| assert!(settings.set_value(section, key, value.clone())));
    settings
}

#[test]
fn serial_ports_are_named_like_actions_firmware_says() {
    let known = vec!["/dev/odd name".to_string()];
    [
        "/dev/ttyACM0",
        "/dev/tty.usbmodem14101",
        "/dev/cu.usbserial-1",
        "/dev/serial/by-id/usb-Arduino_Mega:2560+x",
        "/dev/serial/by-path/platform-3f980000.usb-usb-0:1.2:1.0",
        "COM3",
        "COM255",
        "/dev/odd name",
    ]
    .iter()
    .for_each(|port| assert!(is_serial_port_name(port, &known), "{port}"));
    [
        "/dev/tty",
        "/dev/../etc/passwd",
        "COM",
        "COM1234",
        "COMx",
        "None",
        "/dev/sda",
        "/dev/ttyACM0;rm",
        "ttyACM0",
        "/dev/serial/by-id/",
    ]
    .iter()
    .for_each(|port| assert!(!is_serial_port_name(port, &known), "{port}"));
    assert!(!is_serial_port_name("/dev/../odd", &["/dev/../odd".into()]));
}

#[test]
fn a_setting_with_missing_fields_is_still_described() {
    let settings = loaded(json!({"Maslow Settings": [{"key": "x", "custom": 1}, {"key": "y", "desc": "Y",
        "default": 2.5, "options": [1, "b"]}, {"key": "z", "desc": "Z\ndefault setting: 3", "default": 3}]}));
    let maslow = section(&settings, "maslow", 0.0, false, &[]).unwrap();
    let field = |key: &str| serde_json::to_value(maslow.settings.iter().find(|f| f.key == key).unwrap()).unwrap();
    assert_eq!(
        field("x"),
        json!({"key": "x", "title": "x", "desc": "", "type": "string", "value": "", "custom": true, "numeric": false})
    );
    assert_eq!(field("y")["desc"], "Y\ndefault setting: 2.5");
    assert_eq!(field("y")["options"], json!(["1", "b"]));
    assert_eq!(field("z")["desc"], "Z\ndefault setting: 3");
    assert!(section(&settings, "ground-control", 0.0, false, &[]).is_none());
    assert_eq!(section_title("gpio"), Some("GPIO Settings"));
}

#[test]
fn a_retired_python_setting_stays_in_the_file_but_off_the_screen() {
    let settings = loaded(json!({"WebControl Settings": [{"key": "recordSerial", "type": "bool", "value": 1}]}));
    assert_eq!(settings.value("WebControl Settings", "recordSerial"), Some(&json!(1)));
    let webcontrol = section(&settings, "webcontrol", 0.0, false, &[]).unwrap();
    assert!(webcontrol.settings.iter().all(|field| field.key != "recordSerial"));
    assert!(webcontrol.settings.iter().any(|field| field.key == "webPort"));
}

#[test]
fn the_planner_settings_show_only_for_a_planner_build() {
    let keys = |planner: bool| -> Vec<String> {
        let maslow = section(&Settings::defaults(), "maslow", 51.29, planner, &[]).unwrap();
        maslow.settings.into_iter().map(|field| field.key).filter(|key| key.starts_with("planner")).collect()
    };
    assert_eq!(keys(true), ["plannerAcceleration", "plannerJunctionDeviation"]);
    assert!(keys(false).is_empty());
}

#[test]
fn custom_settings_follow_the_firmware_version() {
    [(0.0, false), (99.9, false), (100.0, true), (101.5, true)].iter().for_each(|(version, enabled)| {
        assert_eq!(section(&Settings::defaults(), "maslow", *version, false, &[]).unwrap().enable_custom, *enabled);
    });
}

#[test]
fn numbers_and_whole_numbers_are_checked_like_api_settings() {
    let advanced = section(&Settings::defaults(), "advanced", 0.0, false, &[]).unwrap();
    let webcontrol = section(&Settings::defaults(), "webcontrol", 0.0, false, &[]).unwrap();
    let ok = |s: &wc_protocol::settings::SettingsSection, key: &str, value: Value| {
        errors(s, &[(key.to_string(), value)].into_iter().collect(), &[]).is_empty()
    };
    [json!("1.5"), json!(" -2e3 "), json!(-2)].into_iter().for_each(|v| assert!(ok(&advanced, "encoderSteps", v)));
    [json!(""), json!("nan"), json!("inf"), json!("1_000"), json!("1e999"), json!("\u{663}")]
        .into_iter()
        .for_each(|v| assert!(!ok(&advanced, "encoderSteps", v.clone()), "{v}"));
    [json!("12"), json!(5000)].into_iter().for_each(|v| assert!(ok(&webcontrol, "webPort", v)));
    [json!("1.5"), json!(false)].into_iter().for_each(|v| assert!(!ok(&webcontrol, "webPort", v.clone()), "{v}"));
    [json!(true), json!(0), json!(1.0)].into_iter().for_each(|v| assert!(ok(&advanced, "truncate", v)));
}

/// Ported from test_api_machine: capabilities from the firmware version and build.
#[test]
fn the_setup_facts_follow_the_firmware() {
    let cases = [
        (0.0, None, false, false),
        (51.27, Some("51.27"), false, false),
        (51.29, Some("51.29x"), false, false),
        (51.29, Some("51.292"), false, true),
        (51.293, Some("51.293"), false, true),
        (100.0, Some("100"), true, false),
    ];
    cases.iter().for_each(|(version, build, four_motor, z_limits)| {
        let setup = machine_setup(&Settings::defaults(), *version, *build);
        assert_eq!((setup.four_motor, setup.firmware_supports_zaxis_limit), (*four_motor, *z_limits), "{build:?}");
        assert_eq!(setup.controller_firmware_version, *version);
    });
    let touch = with(&[
        ("Advanced Settings", "touchPlate", json!(1)),
        ("Advanced Settings", "chainExtendLength", json!("abc")),
    ]);
    let setup = machine_setup(&touch, 1.3, Some("1.3"));
    assert!(setup.touch_plate);
    assert_eq!(setup.chain_extend_length, 0.0);
    assert_eq!(setup.firmware_build.as_deref(), Some("1.3"));
    assert!(machine_setup(&with(&[("Advanced Settings", "touchPlate", json!("0"))]), 0.0, None).touch_plate);
}

/// Ported from test_api_machine: triangular kits read back as Ring or Custom; the chain side defaults to Top.
#[test]
fn quick_configure_reads_the_kit_back() {
    let ring = with(&[
        ("Advanced Settings", "rotationRadius", json!("138.4")),
        ("Maslow Settings", "motorOffsetY", json!(463)),
    ]);
    assert_eq!(
        serde_json::to_value(quick_configure(&ring)).unwrap(),
        json!({"kinematicsType": "Ring", "rotationRadius": 138.4, "chainOverSprocket": "Bottom",
            "motorSpacingX": 2978.4, "motorOffsetY": 463.0})
    );
    assert_eq!(quick_configure(&Settings::defaults()).kinematics_type, "Custom");
    let quad = with(&[
        ("Advanced Settings", "kinematicsType", json!("Quadrilateral")),
        ("Advanced Settings", "chainOverSprocket", json!(null)),
    ]);
    let read = quick_configure(&quad);
    assert_eq!((read.kinematics_type.as_str(), read.chain_over_sprocket.as_str()), ("Quadrilateral", "Top"));
}

#[test]
fn pid_gains_are_the_stored_text() {
    let gains = with(&[("Advanced Settings", "KpV", json!(7)), ("Advanced Settings", "KiV", json!(null))]);
    let pid = serde_json::to_value(pid_settings(&gains, 100.0)).unwrap();
    assert_eq!(pid["velocity"], json!({"kp": "7", "ki": "", "kd": "0.28"}));
    assert_eq!(pid["position"]["kd"], "34");
    assert_eq!(pid["fourMotor"], true);
}
