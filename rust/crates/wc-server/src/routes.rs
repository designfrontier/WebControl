use axum::Json;
use axum::Router;
use axum::extract::Path;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::json;

use crate::{AppState, api, assets, ws};

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
        .route("/ws", get(ws::upgrade))
        .fallback(fallback)
        .with_state(state)
}
