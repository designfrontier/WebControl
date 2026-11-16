//! `/api/calibration/optical*` (Track D): 409 while the setting is off, 501 until each route is built.

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use serde_json::json;
use wc_machine::optical;

use crate::AppState;

pub const NOT_BUILT: &str = "This part of optical calibration is not built yet.";

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/calibration/optical", get(pending))
        .route("/api/calibration/optical/settings", put(pending))
        .route("/api/calibration/optical/scan", post(pending))
        .route("/api/calibration/optical/test-image", post(pending))
        .route("/api/calibration/optical/find-center", post(pending))
        .route("/api/calibration/optical/fit", post(pending))
        .route("/api/calibration/optical/accept", post(pending))
        .route("/api/calibration/optical/csv", get(pending))
        .route("/api/calibration/optical/target.pdf", get(pending))
}

/// The 409 every optical route answers while the setting is off.
pub(crate) fn refused(state: &AppState) -> Option<Response> {
    let off = !optical::enabled(&state.machine.snapshot().settings);
    off.then(|| (StatusCode::CONFLICT, Json(json!({"error": optical::OFF}))).into_response())
}

async fn pending(State(state): State<AppState>) -> Response {
    refused(&state).unwrap_or_else(|| (StatusCode::NOT_IMPLEMENTED, Json(json!({"error": NOT_BUILT}))).into_response())
}
