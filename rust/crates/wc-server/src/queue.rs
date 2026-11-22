//! `/api/queue`: the job queue's routes, and the task that loads its next file and saves its list.

use std::path::{Path, PathBuf};

use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::de::DeserializeOwned;
use serde_json::json;
use tokio::sync::broadcast::error::RecvError;
use wc_config::library::resolve_inside;
use wc_gcode::firmware_gcode::{FileTools, file_tools};
use wc_machine::Request;
use wc_machine::files::{Origin, Problem, Request as Ask};
use wc_machine::queue::{Edit, Item, Outbound};
use wc_protocol::queue::{JobQueue, QueueAdd, QueueContinue, QueueMove, QueueOverride, QueueRemove};

use crate::AppState;
use crate::api::answer;
use crate::files::{detached, load_file};

const FILE: &str = "queue.json";

pub(crate) async fn view(State(state): State<AppState>) -> Json<JobQueue> {
    Json((*state.machine.snapshot().queue).clone())
}

fn body<T: DeserializeOwned>(bytes: &Bytes) -> Result<T, Response> {
    serde_json::from_slice(bytes)
        .map_err(|error| (StatusCode::BAD_REQUEST, Json(json!({ "error": error.to_string() }))).into_response())
}

/// The file's path in the library as the loaded file is named, and its tools; `None` if it cannot be read.
async fn scan(root: PathBuf, file: String) -> Option<Item> {
    let read = tokio::task::spawn_blocking(move || {
        let path = resolve_inside(&root, &[&file]).ok().filter(|path| path.is_file())?;
        let bytes = std::fs::read(&path).ok()?;
        let parts = path.strip_prefix(&root).ok()?.iter().map(|part| part.to_string_lossy()).collect::<Vec<_>>();
        let text = wc_gcode::load::universal_newlines(&String::from_utf8_lossy(&bytes));
        Some(Item { file: parts.join("/"), tools: file_tools(text.lines()), skip_bit_changes: false })
    });
    read.await.ok().flatten()
}

pub(crate) async fn add(State(state): State<AppState>, bytes: Bytes) -> Response {
    let QueueAdd { file, skip_bit_changes } = match body(&bytes) {
        Ok(add) => add,
        Err(refusal) => return refusal,
    };
    match scan(state.paths.gcode_dir(), file).await {
        Some(item) => answer(&state, Request::Queue(Edit::Add(Item { skip_bit_changes, ..item }))).await,
        None => (StatusCode::NOT_FOUND, Json(json!({ "error": "No such file in the gcode library." }))).into_response(),
    }
}

pub(crate) async fn move_item(State(state): State<AppState>, bytes: Bytes) -> Response {
    match body::<QueueMove>(&bytes) {
        Ok(QueueMove { id, to }) => answer(&state, Request::Queue(Edit::Move { id, to })).await,
        Err(refusal) => refusal,
    }
}

pub(crate) async fn override_item(State(state): State<AppState>, bytes: Bytes) -> Response {
    match body::<QueueOverride>(&bytes) {
        Ok(QueueOverride { id, skip_bit_changes }) => {
            answer(&state, Request::Queue(Edit::Override { id, skip: skip_bit_changes })).await
        }
        Err(refusal) => refusal,
    }
}

pub(crate) async fn remove(State(state): State<AppState>, bytes: Bytes) -> Response {
    match body::<QueueRemove>(&bytes) {
        Ok(QueueRemove { id }) => answer(&state, Request::Queue(Edit::Remove { id })).await,
        Err(refusal) => refusal,
    }
}

async fn plain(state: &AppState, bytes: &Bytes, edit: Edit) -> Response {
    match body::<serde_json::Map<String, serde_json::Value>>(bytes) {
        Ok(fields) if fields.is_empty() => answer(state, Request::Queue(edit)).await,
        Ok(_) => (StatusCode::BAD_REQUEST, Json(json!({ "error": "Send an empty object." }))).into_response(),
        Err(refusal) => refusal,
    }
}

pub(crate) async fn clear(State(state): State<AppState>, bytes: Bytes) -> Response {
    plain(&state, &bytes, Edit::Clear).await
}

pub(crate) async fn resume(State(state): State<AppState>, bytes: Bytes) -> Response {
    match body::<QueueContinue>(&bytes) {
        Ok(QueueContinue { from }) => answer(&state, Request::Queue(Edit::Continue(from))).await,
        Err(refusal) => refusal,
    }
}

/// One saved file and its override.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    file: String,
    #[serde(default)]
    skip_bit_changes: bool,
}

/// The list saved before a restart, each file read again for its tools and named as a load names it; one that is
/// gone keeps no tools.
pub async fn restore(dir: &Path) -> Vec<Item> {
    let saved = tokio::fs::read(dir.join(FILE)).await.ok();
    let files: Vec<Saved> = saved.and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
    let root = dir.join("gcode");
    let items = files.into_iter().map(|Saved { file, skip_bit_changes }| {
        let root = root.clone();
        async move {
            match scan(root, file.clone()).await {
                Some(item) => Item { skip_bit_changes, ..item },
                None => Item { file, tools: FileTools::default(), skip_bit_changes },
            }
        }
    });
    futures_util::future::join_all(items).await
}

/// Loads each next file through the same path `/openGCode` takes, and writes the list whenever it changes.
pub fn serve(state: &AppState) -> tokio::task::JoinHandle<()> {
    let (state, mut requests) = (state.clone(), state.machine.queue_requests());
    tokio::spawn(async move {
        loop {
            match requests.recv().await {
                Ok(Outbound::Load(file)) => load(&state, file).await,
                Ok(Outbound::Save(files)) => save(&state.paths.dir(), &files).await,
                Err(RecvError::Lagged(missed)) => tracing::warn!(missed, "the job queue missed requests"),
                Err(RecvError::Closed) => break,
            }
        }
    })
}

async fn load(state: &AppState, file: String) {
    let Ok(target) = resolve_inside(&state.paths.gcode_dir(), &[&file]) else {
        state.machine.submit(Ask::Problem(Problem::GcodeUnreadable));
        return;
    };
    let reply = detached(state, move |loading| load_file(loading, target, None, Origin::Queue)).await;
    tracing::info!(file, ?reply, "queued file loaded");
}

async fn save(dir: &Path, files: &[(String, bool)]) {
    let saved: Vec<Saved> =
        files.iter().map(|(file, skip)| Saved { file: file.clone(), skip_bit_changes: *skip }).collect();
    let text = serde_json::to_vec_pretty(&saved).unwrap_or_default();
    let partial = dir.join(format!(".{FILE}.part"));
    let written = async {
        tokio::fs::write(&partial, text).await?;
        tokio::fs::rename(&partial, dir.join(FILE)).await
    };
    if let Err(error) = written.await {
        tracing::warn!(%error, "could not save the job queue");
    }
}
