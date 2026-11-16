//! The settings screens as `api/settings.py`, `api/machine.py` and `api/system.py` build them from the settings:
//! each section as a form and the checks on a save, the setup facts, quick configure and the PID gains.

use serde_json::{Map, Value};
use wc_config::{Settings, parse_number, python_str};
use wc_firmware::boot::supports_z_limits;
use wc_protocol::machine::{CHAIN_SIDES, MachineSetup, QuickConfigure};
use wc_protocol::settings::{SettingField, SettingsSection, Theme};
use wc_protocol::system::{PidGains, PidSettings};

/// The settings screens disable custom-flagged settings below firmware 100.
pub const CUSTOM_FIRMWARE_VERSION: f64 = 100.0;
const PID_WEIGHTS: [&str; 4] = ["propWeight", "propWeightZ", "propWeightV", "propWeightVZ"];
const PID_GAINS: [&str; 12] =
    ["KpPos", "KiPos", "KdPos", "KpPosZ", "KiPosZ", "KdPosZ", "KpV", "KiV", "KdV", "KpVZ", "KiVZ", "KdVZ"];
pub const SECTIONS: [(&str, &str); 5] = [
    ("maslow", "Maslow Settings"),
    ("advanced", "Advanced Settings"),
    ("webcontrol", "WebControl Settings"),
    ("camera", "Camera Settings"),
    ("gpio", "GPIO Settings"),
];
/// `GPIOActions.actionList`: what a GPIO pin setting may be set to.
pub const GPIO_ACTIONS: [&str; 12] = [
    "Spindle On",
    "Spindle Off",
    "Shutdown",
    "Stop",
    "Pause",
    "Play",
    "Set Home",
    "Go Home",
    "Return to Center",
    "PlayLED",
    "PauseLED",
    "StopLED",
];
const RING_RADIUS: f64 = 138.4;
/// Python-only settings an older `webcontrol.json` still carries; kept in the file, left off the screens.
const RETIRED_KEYS: [&str; 1] = ["recordSerial"];

/// The stored `uiTheme`, which every client shows.
pub fn theme(settings: &Settings) -> Theme {
    Theme::from_setting(&settings.text_or_empty("WebControl Settings", "uiTheme"))
}

pub fn section_title(id: &str) -> Option<&'static str> {
    SECTIONS.iter().find(|(known, _)| *known == id).map(|(_, title)| *title)
}

/// `api/settings.py`'s `is_number`: finite numbers, and text the frontend's number check accepts.
fn is_number(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Number(n)) => n.as_f64().is_some_and(f64::is_finite),
        Some(Value::String(text)) => parse_number(text).is_some(),
        _ => false,
    }
}

/// `int(str(value))` succeeds.
fn is_int(value: &Value) -> bool {
    !value.is_boolean() && python_str(value).trim().parse::<i64>().is_ok()
}

/// `getJSONSettingSection` appends the default to the description, once.
fn description(option: &Map<String, Value>) -> String {
    let desc = option.get("desc").and_then(Value::as_str).unwrap_or_default();
    match option.get("default") {
        Some(default) if option.contains_key("desc") && !desc.contains("default setting:") => {
            format!("{desc}\ndefault setting: {}", python_str(default))
        }
        _ => desc.to_string(),
    }
}

fn describe(section: &str, option: &Map<String, Value>) -> SettingField {
    let text = |name: &str| option.get(name).and_then(Value::as_str).map(String::from);
    let key = text("key").unwrap_or_default();
    let kind = text("type").unwrap_or_else(|| "string".into());
    let value = option.get("value").cloned().unwrap_or_else(|| Value::from(""));
    // A setting not yet read from the controller is blank, but numeric all the same.
    let value_or_default = if value == "" { option.get("default") } else { Some(&value) };
    let options = match section {
        "GPIO Settings" => Some(std::iter::once("").chain(GPIO_ACTIONS).map(String::from).collect()),
        _ => option
            .get("options")
            .and_then(Value::as_array)
            .map(|list| list.iter().map(|o| o.as_str().map(String::from).unwrap_or_else(|| o.to_string())).collect()),
    };
    SettingField {
        title: text("title").unwrap_or_else(|| key.clone()),
        desc: description(option),
        numeric: kind == "int" || kind == "float" || (kind == "string" && is_number(value_or_default)),
        custom: option.get("custom") == Some(&Value::from(1)),
        key,
        kind,
        value,
        options,
    }
}

/// `GET /api/settings/{sectionId}`'s body; `None` for an unknown section.
pub fn section(settings: &Settings, id: &str, firmware_version: f64, ports: &[String]) -> Option<SettingsSection> {
    let title = section_title(id)?;
    Some(SettingsSection {
        id: id.into(),
        title: title.into(),
        enable_custom: firmware_version >= CUSTOM_FIRMWARE_VERSION,
        settings: settings
            .section(title)
            .into_iter()
            .map(|option| describe(title, option))
            .filter(|field| !RETIRED_KEYS.contains(&field.key.as_str()))
            .collect(),
        ports: (id == "maslow").then(|| ports.to_vec()),
    })
}

fn device_name(text: &str, prefix: &str, allowed: impl Fn(char) -> bool) -> bool {
    text.strip_prefix(prefix).is_some_and(|rest| !rest.is_empty() && rest.chars().all(allowed))
}

/// `Actions/firmware.isSerialPortName`.
pub fn is_serial_port_name(text: &str, known: &[String]) -> bool {
    let tty = |c: char| c.is_ascii_alphanumeric() || "._-".contains(c);
    let by = |c: char| c.is_ascii_alphanumeric() || "._:+-".contains(c);
    let com =
        text.strip_prefix("COM").is_some_and(|n| (1..=3).contains(&n.len()) && n.chars().all(|c| c.is_ascii_digit()));
    let named = ["/dev/tty", "/dev/cu"].iter().any(|p| device_name(text, p, tty))
        || ["/dev/serial/by-id/", "/dev/serial/by-path/"].iter().any(|p| device_name(text, p, by))
        || com;
    !text.contains("..") && text != "None" && (known.iter().any(|k| k == text) || named)
}

/// `setting_error`: what is wrong with one posted value, in Python's order of checks.
fn setting_error(field: &SettingField, value: &Value, enable_custom: bool, ports: &[String]) -> Option<&'static str> {
    let in_options = |options: &Vec<String>| value.as_str().is_some_and(|v| options.iter().any(|o| o == v));
    if field.key == "COMport" && value != "" && !is_serial_port_name(&python_str(value), ports) {
        Some("Must be a serial port such as /dev/ttyACM0 or COM3")
    } else if field.custom && !enable_custom {
        Some("Needs custom firmware (version 100 or later)")
    } else if field.kind == "bool" {
        let flag = value.is_boolean() || value.as_f64().is_some_and(|n| n == 0.0 || n == 1.0);
        (!flag).then_some("Must be true or false")
    } else if !(value.is_string() || value.is_number()) {
        Some("Must be text or a number")
    } else if field.options.as_ref().is_some_and(|options| !in_options(options)) {
        Some("Must be one of the listed options")
    } else if field.kind == "int" && !is_int(value) {
        Some("Must be a whole number")
    } else if field.numeric && !is_number(Some(value)) {
        Some("Must be a number")
    } else {
        pid_error(&field.key, value).or_else(|| pacing_error(&field.key, value))
    }
}

fn pacing_error(key: &str, value: &Value) -> Option<&'static str> {
    let ms = wc_config::python_float(value)?;
    (key == crate::pacing::GAP_KEY && !(0.0..=crate::pacing::MAX_GAP_MS as f64).contains(&ms))
        .then_some("Must be between 0 and 500")
}

/// `PID::SetTunings` ignores a tuning with a weight outside [0, 1] or a negative gain.
fn pid_error(key: &str, value: &Value) -> Option<&'static str> {
    let number = wc_config::python_float(value)?;
    if PID_WEIGHTS.contains(&key) {
        (!(0.0..=1.0).contains(&number)).then_some("Must be between 0 and 1")
    } else {
        (PID_GAINS.contains(&key) && number < 0.0).then_some("Must be 0 or more")
    }
}

/// `validate`: every unknown or invalid posted value, by key.
pub fn errors(section: &SettingsSection, values: &Map<String, Value>, ports: &[String]) -> Map<String, Value> {
    values
        .iter()
        .filter_map(|(key, value)| {
            let error = match section.settings.iter().find(|field| &field.key == key) {
                None => Some("Unknown setting"),
                Some(field) => setting_error(field, value, section.enable_custom, ports),
            };
            error.map(|error| (key.clone(), Value::from(error)))
        })
        .collect()
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
    }
}

/// `form_value`: what the old HTML form would have posted, so the save sees Python's input.
pub fn form_value(field: &SettingField, value: &Value) -> Value {
    match field.kind.as_str() {
        "bool" => Value::Bool(truthy(value)),
        _ => Value::String(python_str(value)),
    }
}

/// `api/machine.py`'s `_number`: `float()` of the value, else 0.
fn number(settings: &Settings, section: &str, key: &str) -> f64 {
    settings.value(section, key).and_then(wc_config::python_float).unwrap_or(0.0)
}

/// `GET /api/machine/quick-configure`: a triangular kit reads back as Ring or Custom by its radius.
pub fn quick_configure(settings: &Settings) -> QuickConfigure {
    let rotation_radius = number(settings, "Advanced Settings", "rotationRadius");
    let text = |key: &str| settings.value("Advanced Settings", key).and_then(Value::as_str);
    let kinematics_type = match text("kinematicsType") {
        Some("Quadrilateral") => "Quadrilateral",
        _ if (rotation_radius - RING_RADIUS).abs() < 0.01 => "Ring",
        _ => "Custom",
    };
    QuickConfigure {
        kinematics_type: kinematics_type.into(),
        rotation_radius,
        chain_over_sprocket: text("chainOverSprocket")
            .filter(|side| CHAIN_SIDES.contains(side))
            .unwrap_or("Top")
            .into(),
        motor_spacing_x: number(settings, "Maslow Settings", "motorSpacingX"),
        motor_offset_y: number(settings, "Maslow Settings", "motorOffsetY"),
    }
}

/// `GET /api/machine/setup`; `touchPlate` is Python's `bool()` of the stored value.
pub fn machine_setup(settings: &Settings, firmware_version: f64, firmware_build: Option<&str>) -> MachineSetup {
    MachineSetup {
        chain_extend_length: number(settings, "Advanced Settings", "chainExtendLength"),
        four_motor: firmware_version >= CUSTOM_FIRMWARE_VERSION,
        firmware_supports_zaxis_limit: supports_z_limits(firmware_build),
        touch_plate: settings.value("Advanced Settings", "touchPlate").is_some_and(truthy),
        controller_firmware_version: firmware_version,
        firmware_build: firmware_build.map(String::from),
    }
}

/// `GET /api/system/pid`.
pub fn pid_settings(settings: &Settings, firmware_version: f64) -> PidSettings {
    let gains = |kp: &str, ki: &str, kd: &str| {
        let text = |key: &str| {
            settings.value("Advanced Settings", key).filter(|v| !v.is_null()).map(python_str).unwrap_or_default()
        };
        PidGains { kp: text(kp), ki: text(ki), kd: text(kd) }
    };
    PidSettings {
        velocity: gains("KpV", "KiV", "KdV"),
        position: gains("KpPos", "KiPos", "KdPos"),
        four_motor: firmware_version >= CUSTOM_FIRMWARE_VERSION,
    }
}
