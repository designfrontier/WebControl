use axum::Json;
use axum::Router;
use axum::extract::{DefaultBodyLimit, Path};
use axum::http::{Method, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use serde_json::json;

use crate::forms::UPLOAD_LIMIT;
use crate::{AppState, api, archive, assets, boards, files, help, optical, updates, ws};

/// The UI's old `/next` addresses: the same path and query at the root.
async fn from_next(uri: Uri) -> Response {
    // Browsers read a leading `/\` as `//`, which would send `/next/\evil.com` off-site.
    let path = uri.path().strip_prefix("/next").unwrap_or_default().trim_start_matches(['/', '\\']);
    let to = uri.query().map_or(format!("/{path}"), |query| format!("/{path}?{query}"));
    (StatusCode::PERMANENT_REDIRECT, [(header::LOCATION, to)]).into_response()
}

/// `/api` errors are JSON, including the 404s Python answered with HTML (wart 9); any other GET is the UI.
async fn fallback(method: Method, uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    match method {
        _ if path.starts_with("api/") => (StatusCode::NOT_FOUND, Json(json!({"error": "Not found"}))).into_response(),
        Method::GET | Method::HEAD => assets::page(path),
        _ => assets::not_found_html(),
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/next", get(from_next))
        .route("/next/", get(from_next))
        .route("/next/{*path}", get(from_next))
        .route("/static/{*filename}", get(|Path(name): Path<String>| async move { assets::image(&name) }))
        .route("/api/state", get(api::state))
        .route("/api/main", get(api::main_screen))
        .route("/api/run/pacing", get(api::pacing).put(api::update_pacing))
        .route("/api/display/theme", put(api::update_theme))
        .route("/api/viewer", get(api::viewer))
        .route("/api/settings/{section}", get(api::settings_section).put(api::update_settings))
        .route("/api/machine/setup", get(api::machine_setup))
        .route("/api/machine/quick-configure", get(api::quick_configure).post(api::save_quick_configure))
        .route("/api/system/info", get(api::system))
        .route("/api/system/pid", get(api::pid))
        .route("/api/system/updates", get(updates::updates))
        .route("/api/system/help", get(help::page))
        .route("/api/system/help/assets/{*path}", get(|Path(path): Path<String>| async move { help::asset(&path) }))
        .route("/api/system/pid-test", post(api::pid_test))
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
        .route("/api/calibration/triangular", get(api::triangular).post(api::calculate_triangular))
        .route("/api/calibration/triangular/accept", post(api::accept_triangular))
        .route("/api/calibration/holey", get(api::holey).post(api::calculate_holey))
        .route("/api/calibration/holey/accept", post(api::accept_holey))
        .merge(optical::routes())
        .route("/ws", get(ws::upgrade))
        .fallback(fallback)
        .layer(DefaultBodyLimit::max(UPLOAD_LIMIT))
        .with_state(state)
}
