//! `/api/tools`, the tool library. Edits are allowed during a job: they only change how tool-change prompts are
//! labelled, never what the controller does.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use wc_config::tools::{self, Refusal};
use wc_protocol::tools::ToolLibrary;
use wc_protocol::{Envelope, Push};

use crate::AppState;

fn refused(status: StatusCode, error: &str) -> Response {
    (status, Json(json!({ "error": error }))).into_response()
}

fn refusal(refusal: &Refusal) -> Response {
    let status = match refusal {
        Refusal::Invalid(_) => StatusCode::BAD_REQUEST,
        Refusal::Taken(_) => StatusCode::CONFLICT,
        Refusal::Missing(_) => StatusCode::NOT_FOUND,
    };
    refused(status, &refusal.to_string())
}

enum Failure {
    Refused(Refusal),
    Io(String),
}

type Edit = Box<dyn FnOnce(&ToolLibrary) -> Result<ToolLibrary, Refusal> + Send>;

/// Loads, edits and saves under the library's lock, then tells every client.
async fn change(state: &AppState, status: StatusCode, edit: Edit) -> Response {
    let (paths, lock) = (state.paths.clone(), state.tools.clone());
    let changed = tokio::task::spawn_blocking(move || {
        let _held = lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let library = tools::load(&paths).map_err(|e| Failure::Io(e.to_string()))?;
        let edited = edit(&library).map_err(Failure::Refused)?;
        tools::save(&paths, &edited).map_err(|e| Failure::Io(e.to_string()))?;
        Ok(edited)
    })
    .await
    .expect("the edit does not panic");
    match changed {
        Ok(library) => {
            state.machine.broadcast(Push::message(Envelope::json(&library)));
            (status, Json(library)).into_response()
        }
        Err(Failure::Refused(why)) => refusal(&why),
        Err(Failure::Io(error)) => {
            tracing::warn!(%error, "the tool library could not be saved");
            refused(StatusCode::INTERNAL_SERVER_ERROR, &format!("The tool library could not be saved: {error}"))
        }
    }
}

fn body(bytes: &Bytes) -> Value {
    serde_json::from_slice(bytes).unwrap_or(Value::Null)
}

fn number(text: &str) -> Result<u32, Response> {
    text.parse().map_err(|_| refused(StatusCode::NOT_FOUND, &format!("Your tool library has no tool {text}.")))
}

pub(crate) async fn library(State(state): State<AppState>) -> Response {
    let paths = state.paths.clone();
    match tokio::task::spawn_blocking(move || tools::load(&paths)).await.expect("loading does not panic") {
        Ok(library) => Json(library).into_response(),
        Err(error) => {
            refused(StatusCode::INTERNAL_SERVER_ERROR, &format!("The tool library could not be read: {error}"))
        }
    }
}

pub(crate) async fn add(State(state): State<AppState>, bytes: Bytes) -> Response {
    match tools::parse_tool(&body(&bytes)) {
        Ok(tool) => change(&state, StatusCode::CREATED, Box::new(move |library| tools::add(library, tool))).await,
        Err(why) => refusal(&why),
    }
}

pub(crate) async fn replace(State(state): State<AppState>, Path(text): Path<String>, bytes: Bytes) -> Response {
    let edit =
        number(&text).and_then(|n| tools::parse_tool(&body(&bytes)).map(|tool| (n, tool)).map_err(|w| refusal(&w)));
    match edit {
        Ok((n, tool)) => {
            change(&state, StatusCode::OK, Box::new(move |library| tools::replace(library, n, tool))).await
        }
        Err(response) => response,
    }
}

pub(crate) async fn remove(State(state): State<AppState>, Path(text): Path<String>) -> Response {
    match number(&text) {
        Ok(n) => change(&state, StatusCode::OK, Box::new(move |library| tools::remove(library, n))).await,
        Err(response) => response,
    }
}
