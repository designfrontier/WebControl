use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::json;
use wc_protocol::main::{Backend, Macro, Point, StartupState, Units};

use crate::{AppState, assets, ws};

fn redirect(status: StatusCode, to: &'static str) -> Response {
    (status, [(header::LOCATION, to)]).into_response()
}

/// `api/state.py`, with the machine's own fields from its snapshot.
async fn api_state(State(state): State<AppState>) -> Json<StartupState> {
    let (settings, snapshot) = (&state.settings, state.machine.snapshot());
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
        host_address: snapshot.host_address,
        controller_firmware_version: snapshot.controller_firmware_version,
        controller_board: snapshot.controller_board,
        firmware_build: snapshot.firmware_build,
        macros: (1..=2)
            .map(|n| Macro::new(n, &settings.text_or_empty("Maslow Settings", &format!("macro{n}_title"))))
            .collect(),
        backend: Backend::Rust,
    })
}

/// `/api` errors are JSON, including the 404s Python answered with HTML (wart 9).
async fn fallback(uri: Uri) -> Response {
    match uri.path().starts_with("/api/") {
        true => (StatusCode::NOT_FOUND, Json(json!({"error": "Not found"}))).into_response(),
        false => assets::not_found_html(),
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(|| async { redirect(StatusCode::FOUND, "/next/") }))
        .route("/next", get(|| async { redirect(StatusCode::PERMANENT_REDIRECT, "/next/") }))
        .route("/next/", get(|| async { assets::next("") }))
        .route("/next/{*path}", get(|Path(path): Path<String>| async move { assets::next(&path) }))
        .route("/static/{*filename}", get(|Path(name): Path<String>| async move { assets::image(&name) }))
        .route("/api/state", get(api_state))
        .route("/ws", get(ws::upgrade))
        .fallback(fallback)
        .with_state(state)
}
