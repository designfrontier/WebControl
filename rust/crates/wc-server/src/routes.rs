use axum::Json;
use axum::Router;
use axum::extract::{DefaultBodyLimit, Path};
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde_json::json;

use crate::forms::UPLOAD_LIMIT;
use crate::{AppState, api, archive, assets, boards, files, ws};

fn redirect(status: StatusCode, to: &'static str) -> Response {
    (status, [(header::LOCATION, to)]).into_response()
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
        .route("/api/state", get(api::state))
        .route("/api/main", get(api::main_screen))
        .route("/api/viewer", get(api::viewer))
        .route("/api/settings/{section}", get(api::settings_section))
        .route("/api/system/info", get(api::system))
        .route("/api/files/gcode", get(files::library))
        .route("/api/files/gcode/directories", post(files::new_directory))
        .route("/api/files/gcode/loaded", get(files::loaded_gcode))
        .route("/api/files/gcode/text", get(files::text))
        .route("/api/files/gcode/custom", get(files::custom))
        .route("/api/boards/files", get(boards::library))
        .route("/api/boards/directories", post(boards::new_directory))
        .route("/api/boards/current", get(boards::current_board))
        .route("/openGCode", post(files::open))
        .route("/uploadGCode", post(files::upload))
        .route("/saveGCode", post(files::save))
        .route("/editGCode", post(files::edit))
        .route("/sendGCode", post(files::send))
        .route("/importFile", post(archive::import_ini))
        .route("/importFileWCJSON", post(archive::import_webcontrol))
        .route("/importRestoreWebControl", post(archive::restore_upload))
        .route("/openBoard", post(boards::open))
        .route("/saveBoard", post(boards::save))
        .route("/editBoard", post(boards::edit))
        .route("/trimBoard", post(boards::trim))
        .route("/downloadDiagnostics", get(archive::download_diagnostics))
        .route("/backupWebControl", get(archive::download_backup))
        .route("/ws", get(ws::upgrade))
        .fallback(fallback)
        .layer(DefaultBodyLimit::max(UPLOAD_LIMIT))
        .with_state(state)
}
