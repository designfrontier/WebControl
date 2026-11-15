//! The `/api` GETs: each built from one machine snapshot, as `api/*.py` builds them from `data`.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Map, Value, json};
use wc_config::{Settings, parse_number};
use wc_firmware::boot::supports_z_limits;
use wc_protocol::main::{Backend, GcodeLine, Macro, MainScreen, Point, StartupState, Units};
use wc_protocol::settings::{SettingField, SettingsSection};
use wc_protocol::system::{BundledFirmware, SystemInfo, UpdateStatus};
use wc_protocol::viewer::{MachineFrame, ViewerSnapshot};

use crate::AppState;

const MM_PER_INCH: f64 = 25.4;
/// The settings screens disable custom-flagged settings below firmware 100.
const CUSTOM_FIRMWARE_VERSION: f64 = 100.0;
const SECTIONS: [(&str, &str); 5] = [
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

/// `api/state.py`.
pub(crate) async fn state(State(state): State<AppState>) -> Json<StartupState> {
    let snapshot = state.machine.snapshot();
    let settings = &snapshot.settings;
    let units = |key: &str| Units::from_setting(&settings.text_or_empty("Computed Settings", key));
    Json(StartupState {
        units: units("units"),
        dist_to_move: settings.number("Computed Settings", "distToMove"),
        units_z: units("unitsZ"),
        dist_to_move_z: settings.number("Computed Settings", "distToMoveZ"),
        home_position: Point {
            x: settings.number("Advanced Settings", "homeX"),
            y: settings.number("Advanced Settings", "homeY"),
        },
        pause_button_setting: snapshot.pause_button,
        job_in_progress: snapshot.job_in_progress,
        manual_z_axis_adjust: snapshot.manual_z_axis_adjust,
        host_address: snapshot.host_address.clone(),
        controller_firmware_version: snapshot.controller_firmware_version,
        controller_board: snapshot.controller_board.clone(),
        firmware_build: snapshot.firmware_build.clone(),
        macros: (1..=2)
            .map(|n| Macro::new(n, &settings.text_or_empty("Maslow Settings", &format!("macro{n}_title"))))
            .collect(),
        backend: Backend::Rust,
    })
}

/// `api/main.py`'s `_flag`: a number's truth, else the text `true`.
fn flag(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|n| n.trunc() != 0.0),
        Some(Value::String(text)) => match text.trim().parse::<f64>() {
            Ok(n) => n.is_finite() && n.trunc() != 0.0,
            Err(_) => text.trim().eq_ignore_ascii_case("true"),
        },
        _ => false,
    }
}

pub(crate) async fn main_screen(State(state): State<AppState>) -> Json<MainScreen> {
    let snapshot = state.machine.snapshot();
    let index = if snapshot.gcode_index < snapshot.gcode_count { snapshot.gcode_index } else { 0 };
    Json(MainScreen {
        status: snapshot.status(),
        touch_plate: flag(snapshot.settings.value("Advanced Settings", "touchPlate")),
        gcode: GcodeLine { line_index: index, line: snapshot.gcode_line.clone(), line_count: snapshot.gcode_count },
    })
}

fn inches(settings: &Settings, key: &str) -> Option<f64> {
    let mm = wc_protocol::float(settings.value("Maslow Settings", key)?)?;
    format!("{:.4}", mm / MM_PER_INCH).parse().ok()
}

fn base64(bytes: Option<&[u8]>) -> Option<String> {
    use base64::Engine;
    bytes.filter(|b| !b.is_empty()).map(|b| base64::engine::general_purpose::STANDARD.encode(b))
}

/// `api/viewer.py`: everything the 3D view draws, in inches, for the client that asks.
pub(crate) async fn viewer(State(state): State<AppState>) -> Json<ViewerSnapshot> {
    let snapshot = state.machine.snapshot();
    let settings = &snapshot.settings;
    Json(ViewerSnapshot {
        machine: MachineFrame {
            bed_width: inches(settings, "bedWidth"),
            bed_height: inches(settings, "bedHeight"),
            motor_offset_y: inches(settings, "motorOffsetY"),
            motor_spacing_x: inches(settings, "motorSpacingX"),
        },
        gcode: base64(snapshot.preview.compressed.as_deref()),
        board: Some(snapshot.board.info()),
        cut_data: base64(snapshot.board.cut_payload().as_deref()),
    })
}

/// `api/settings.py`'s `is_number`: finite numbers, and text the frontend's number check accepts.
fn is_number(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Number(n)) => n.as_f64().is_some_and(f64::is_finite),
        Some(Value::String(text)) => parse_number(text).is_some(),
        _ => false,
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
        desc: text("desc").unwrap_or_default(),
        numeric: kind == "int" || kind == "float" || (kind == "string" && is_number(value_or_default)),
        custom: option.get("custom") == Some(&Value::from(1)),
        key,
        kind,
        value,
        options,
    }
}

/// `GET /api/settings/{sectionId}`; an unknown section is a JSON 404 (wart 9).
pub(crate) async fn settings_section(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let Some((id, section)) = SECTIONS.iter().find(|(known, _)| *known == id) else {
        return (StatusCode::NOT_FOUND, Json(json!({"error": "Not found"}))).into_response();
    };
    let snapshot = state.machine.snapshot();
    Json(SettingsSection {
        id: id.to_string(),
        title: section.to_string(),
        enable_custom: snapshot.controller_firmware_version >= CUSTOM_FIRMWARE_VERSION,
        settings: snapshot.settings.section(section).into_iter().map(|option| describe(section, option)).collect(),
        ports: (*id == "maslow").then(|| snapshot.ports.clone()),
    })
    .into_response()
}

pub(crate) async fn system(State(state): State<AppState>) -> Json<SystemInfo> {
    let snapshot = state.machine.snapshot();
    let version = snapshot.controller_firmware_version;
    Json(SystemInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        platform: state.platform.name.clone(),
        controller_firmware_version: version,
        firmware: BundledFirmware { stock: None, holey: None, custom: None },
        can_turn_off: state.platform.can_turn_off,
        four_motor: version >= CUSTOM_FIRMWARE_VERSION,
        firmware_supports_zaxis_limit: supports_z_limits(snapshot.firmware_build.as_deref()),
        firmware_build: snapshot.firmware_build.clone(),
        update: UpdateStatus { available: false, version: None },
    })
}
