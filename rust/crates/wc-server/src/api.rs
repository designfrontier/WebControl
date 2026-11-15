//! The `/api` GETs: each built from one machine snapshot, as `api/*.py` builds them from `data`.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use wc_config::Settings;
use wc_firmware::boot::supports_z_limits;
use wc_machine::forms::{self, CUSTOM_FIRMWARE_VERSION};
use wc_machine::{Request, Response as Answer};
use wc_protocol::machine::{MachineSetup, QuickConfigure};
use wc_protocol::main::{Backend, GcodeLine, Macro, MainScreen, Point, StartupState, Units};
use wc_protocol::system::{BundledFirmware, PidSettings, SystemInfo, UpdateStatus};
use wc_protocol::viewer::{MachineFrame, ViewerSnapshot};

use crate::AppState;

const MM_PER_INCH: f64 = 25.4;

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

/// `GET /api/settings/{sectionId}`; an unknown section is a JSON 404 (wart 9).
pub(crate) async fn settings_section(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let snapshot = state.machine.snapshot();
    match forms::section(&snapshot.settings, &id, snapshot.controller_firmware_version, &snapshot.ports) {
        Some(section) => Json(section).into_response(),
        None => not_found(),
    }
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, Json(json!({"error": "Not found"}))).into_response()
}

/// The body as JSON, or null where Flask's `get_json(silent=True)` gives None.
fn json_body(body: &Bytes) -> Value {
    serde_json::from_slice(body).unwrap_or(Value::Null)
}

async fn answer(state: &AppState, request: Request) -> Response {
    match state.machine.edit(request).await {
        Some(Answer { status, body }) => {
            (StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR), Json(body)).into_response()
        }
        None => {
            (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "WebControl is shutting down"}))).into_response()
        }
    }
}

/// `PUT /api/settings/{sectionId}`.
pub(crate) async fn update_settings(State(state): State<AppState>, Path(id): Path<String>, body: Bytes) -> Response {
    answer(&state, Request::UpdateSettings { section: id, body: json_body(&body) }).await
}

pub(crate) async fn machine_setup(State(state): State<AppState>) -> Json<MachineSetup> {
    let snapshot = state.machine.snapshot();
    let build = snapshot.firmware_build.as_deref();
    Json(forms::machine_setup(&snapshot.settings, snapshot.controller_firmware_version, build))
}

pub(crate) async fn quick_configure(State(state): State<AppState>) -> Json<QuickConfigure> {
    Json(forms::quick_configure(&state.machine.snapshot().settings))
}

pub(crate) async fn save_quick_configure(State(state): State<AppState>, body: Bytes) -> Response {
    answer(&state, Request::QuickConfigure(json_body(&body))).await
}

pub(crate) async fn pid(State(state): State<AppState>) -> Json<PidSettings> {
    let snapshot = state.machine.snapshot();
    Json(forms::pid_settings(&snapshot.settings, snapshot.controller_firmware_version))
}

/// `POST /api/system/pid-test`; a body that is not JSON reads as `{}`, as Flask's `or {}` does.
pub(crate) async fn pid_test(State(state): State<AppState>, body: Bytes) -> Response {
    let body = Some(json_body(&body)).filter(|b| !b.is_null()).unwrap_or_else(|| json!({}));
    answer(&state, Request::PidTest(body)).await
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
