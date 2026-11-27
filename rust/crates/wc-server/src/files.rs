//! The gcode library and the loaded gcode: `api/files.py` and the classic gcode form routes in `main.py`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use wc_config::Settings;
use wc_config::library::{LIBRARY_ROOT, Listing, directory_setting, list_library, resolve_inside, secure_filename};
use wc_machine::files::{GCODE_LOCKED, Gcode, Origin, Problem, Reply, Request as Ask};
use wc_machine::{Event, MachineHandle};
use wc_protocol::files::{
    ApiError, CreatedDirectory, DirectoryRequest, GcodeDelete, GcodeText, Library, LibraryFile, LibraryListing,
    LoadedGcode,
};
use wc_protocol::viewer::LastRun;

use crate::AppState;
use crate::forms::{self, Form};
use crate::memory::{self, Fit};

const NO_NAME: &str = "Enter a directory name using letters, numbers, dashes or underscores.";
const OUTSIDE: &str = "That directory points outside the library.";
const NO_PATH: &str = "Send {\"path\": \"<directory>/<file name>\"} or {\"path\": \"<directory>\"}.";
const DELETE_OUTSIDE: &str = "That path points outside the library.";
const TOP_LEVEL: &str = "The top level of the library cannot be deleted.";
const MISSING: &str = "No such file or directory in the gcode library.";

pub(crate) fn files(listing: Listing) -> (Vec<String>, Vec<LibraryFile>) {
    let files = listing.files.into_iter().map(|f| LibraryFile { directory: f.directory, name: f.name }).collect();
    (listing.directories, files)
}

fn api_error(status: StatusCode, error: &str) -> Response {
    (status, Json(ApiError { error: error.into() })).into_response()
}

/// `create_directory`: the body's name made one safe path segment; the name made, or the refusal.
pub(crate) fn create_directory(root: &Path, body: &[u8]) -> Result<(String, Response), Response> {
    let name = secure_filename(DirectoryRequest::from_body(body).name.trim());
    if name.is_empty() {
        return Err(api_error(StatusCode::BAD_REQUEST, NO_NAME));
    }
    let path = resolve_inside(root, &[&name]).map_err(|_| api_error(StatusCode::BAD_REQUEST, OUTSIDE))?;
    std::fs::create_dir_all(&path).map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
    let (directories, files) = files(list_library(root));
    let created = CreatedDirectory { directory: name.clone(), directories, files };
    Ok((name, (StatusCode::CREATED, Json(created)).into_response()))
}

pub(crate) async fn library(State(state): State<AppState>) -> Json<Library> {
    let settings = state.machine.snapshot().settings;
    let (directories, files) = files(list_library(&state.paths.gcode_dir()));
    Json(Library {
        directories,
        files,
        last_directory: directory_setting(&settings.text_or_empty("Computed Settings", "lastSelectedDirectory")),
        last_file: settings.text_or_empty("Maslow Settings", "openFile"),
        valid_extensions: Some(settings.text_or_empty("WebControl Settings", "validExtensions")),
    })
}

pub(crate) async fn new_directory(State(state): State<AppState>, body: Bytes) -> Response {
    match create_directory(&state.paths.gcode_dir(), &body) {
        Ok((name, response)) => {
            state.machine.send(Event::DirectoryCreated(name));
            response
        }
        Err(refusal) => refusal,
    }
}

/// The loaded file and every queued file, as paths under `root`.
fn in_use(state: &AppState, root: &Path) -> Vec<PathBuf> {
    let snapshot = state.machine.snapshot();
    let loaded = snapshot.gcode_file.filter(|_| !snapshot.gcode.is_empty());
    let queued = snapshot.queue.items.iter().filter_map(|item| resolve_inside(root, &[&item.file]).ok());
    loaded.into_iter().chain(queued).collect()
}

/// Symlinks resolved, so two spellings of one file compare equal; a missing path stays as given.
fn real(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Deletes a file, or a directory with everything in it, unless it is or holds a file in use.
pub(crate) async fn delete(State(state): State<AppState>, body: Bytes) -> Response {
    let Ok(GcodeDelete { path }) = serde_json::from_slice(&body) else {
        return api_error(StatusCode::BAD_REQUEST, NO_PATH);
    };
    let root = state.paths.gcode_dir();
    let Ok(target) = resolve_inside(&root, &[&path]) else {
        return api_error(StatusCode::BAD_REQUEST, DELETE_OUTSIDE);
    };
    if real(&target) == real(&root) {
        return api_error(StatusCode::BAD_REQUEST, TOP_LEVEL);
    }
    if in_use(&state, &root).iter().any(|used| real(used).starts_with(real(&target))) {
        let refusal = format!("{path} is, or holds, the loaded gcode or a file in the job queue, so it stays.");
        return api_error(StatusCode::CONFLICT, &refusal);
    }
    let removed = match tokio::fs::symlink_metadata(&target).await {
        Ok(found) if found.is_dir() => tokio::fs::remove_dir_all(&target).await,
        Ok(_) => tokio::fs::remove_file(&target).await,
        Err(_) => return api_error(StatusCode::NOT_FOUND, MISSING),
    };
    if let Err(error) = removed {
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string());
    }
    let (directories, files) = files(list_library(&root));
    Json(LibraryListing { directories, files }).into_response()
}

/// `loaded_gcode`: where the loaded file sits in the library; outside it, or nested deeper, has no directory.
pub fn loaded(lines: usize, file: Option<&Path>, root: &Path) -> LoadedGcode {
    let Some(file) = file.filter(|_| lines > 0) else {
        return LoadedGcode { directory: None, name: None, line_count: lines };
    };
    let directory = file.strip_prefix(root).ok().and_then(|relative| {
        let parent = relative.parent().unwrap_or(Path::new(""));
        (parent.components().count() <= 1).then(|| match parent.to_string_lossy().into_owned() {
            empty if empty.is_empty() => LIBRARY_ROOT.to_string(),
            directory => directory,
        })
    });
    let name = file.file_name().map(|name| name.to_string_lossy().into_owned());
    LoadedGcode { directory, name, line_count: lines }
}

pub(crate) async fn loaded_gcode(State(state): State<AppState>) -> Json<LoadedGcode> {
    let snapshot = state.machine.snapshot();
    Json(loaded(snapshot.gcode.len(), snapshot.gcode_file.as_deref(), &state.paths.gcode_dir()))
}

pub(crate) async fn text(State(state): State<AppState>) -> Json<GcodeText> {
    Json(GcodeText { text: state.machine.snapshot().gcode.join("\n").trim_end().to_string() })
}

pub(crate) async fn custom(State(state): State<AppState>) -> Json<GcodeText> {
    Json(GcodeText { text: state.machine.snapshot().custom_gcode.unwrap_or_default() })
}

fn locked(state: &AppState) -> Option<Response> {
    state.machine.snapshot().gcode_locked.then(|| forms::refused(409, GCODE_LOCKED))
}

/// `Text` is a load that went ahead with a warning.
fn reply_status(reply: Reply) -> Response {
    match reply {
        Reply::Done => forms::status(200),
        Reply::Text(warning) => forms::warned(&warning),
        Reply::Refused(error) => forms::refused(409, &error),
        _ => forms::status(500),
    }
}

async fn parse(text: String, settings: wc_machine::Shared<Settings>) -> Option<Gcode> {
    let parsed =
        tokio::task::spawn_blocking(move || Gcode::parse(&wc_gcode::load::universal_newlines(&text), &settings));
    match parsed.await {
        Ok(Ok(gcode)) => Some(gcode),
        Ok(Err(error)) => {
            tracing::warn!(%error, "could not load the gcode");
            None
        }
        Err(error) => {
            tracing::error!(%error, "the gcode parser stopped");
            None
        }
    }
}

async fn read(path: PathBuf) -> std::io::Result<String> {
    tokio::fs::read(path).await.map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// The file the last job started, drawn afresh from disk; `null` when there is none or it no longer reads.
pub(crate) async fn last_run(State(state): State<AppState>) -> Json<Option<LastRun>> {
    let settings = state.machine.snapshot().settings;
    let file = settings.text_or_empty("Computed Settings", "lastRunFile");
    let text = match file.is_empty() {
        true => None,
        false => read(file.into()).await.ok(),
    };
    let gcode = match text {
        Some(text) => parse(text, settings.clone()).await,
        None => None,
    };
    Json(gcode.and_then(|gcode| {
        Some(LastRun {
            gcode: crate::api::base64(gcode.preview.compressed.as_deref())?,
            home_x: settings.number("Computed Settings", "lastRunHomeX"),
            home_y: settings.number("Computed Settings", "lastRunHomeY"),
        })
    }))
}

/// A load the machine has heard begin (wart 14: Play waits, the spinners start). Dropped unfinished, it ends as
/// unreadable, so no load that stopped part way holds Play.
pub(crate) struct Loading {
    machine: MachineHandle,
    finished: bool,
}

impl Loading {
    async fn begin(machine: MachineHandle) -> Loading {
        machine.request(Ask::LoadStarting).await;
        Loading { machine, finished: false }
    }

    async fn finish(mut self, request: Ask) -> Reply {
        self.finished = true;
        self.machine.request(request).await
    }

    async fn refused(self, why: String) -> Reply {
        self.finish(Ask::LoadRefused(why)).await
    }

    async fn unreadable(self) -> Reply {
        self.finish(Ask::Problem(Problem::GcodeUnreadable)).await;
        Reply::Failed
    }
}

impl Drop for Loading {
    fn drop(&mut self) {
        if !self.finished {
            self.machine.submit(Ask::Problem(Problem::GcodeUnreadable));
        }
    }
}

/// Runs `load` on a task of its own, so a client that disconnects mid-load cannot cancel it.
pub(crate) async fn detached<L, F>(state: &AppState, load: L) -> Reply
where
    L: FnOnce(Loading) -> F + Send + 'static,
    F: Future<Output = Reply> + Send,
{
    let (machine, queue) = (state.machine.clone(), state.loads.clone());
    let (answer, reply) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let loading = Loading::begin(machine.clone()).await;
        let _turn = queue.lock().await;
        if answer.send(load(loading).await).is_err() {
            // The client that would ask for the preview has gone, so every client is sent it.
            machine.send(Event::CheckForGcodeUpdate);
        }
    });
    reply.await.unwrap_or(Reply::Failed)
}

/// `loadUpdateFile` from disk: the file read and parsed off the machine task, then handed over.
pub(crate) async fn load_file(
    loading: Loading,
    file: PathBuf,
    text: Option<String>,
    origin: Origin,
    free_memory: fn() -> Option<u64>,
) -> Reply {
    let text = match text {
        Some(text) => Ok(text),
        None => read(file.clone()).await,
    };
    let fit = text.as_ref().map_or(Fit::Fine, |text| memory::fit(text.lines().count(), free_memory()));
    if let Fit::Refuse(why) = fit {
        tracing::warn!(file = %file.display(), why, "refused a gcode file too large for the free memory");
        return loading.refused(why).await;
    }
    let gcode = match text {
        Ok(text) => parse(text, loading.machine.snapshot().settings).await,
        Err(error) => {
            tracing::warn!(%error, file = %file.display(), "could not read the gcode");
            None
        }
    };
    let reply = match gcode {
        Some(gcode) => loading.finish(Ask::LoadGcode { gcode: Box::new(gcode), file: Some(file), origin }).await,
        None => loading.unreadable().await,
    };
    match (reply, fit) {
        (Reply::Done, Fit::Warn(warning)) => Reply::Text(warning),
        (reply, _) => reply,
    }
}

fn setting(section: &str, key: &str, value: &str) -> (String, String, serde_json::Value) {
    (section.into(), key.into(), value.into())
}

pub(crate) async fn open(State(state): State<AppState>, request: Request) -> Response {
    if let Some(refusal) = locked(&state) {
        return refusal;
    }
    let form = match Form::read(request).await {
        Ok(form) => form,
        Err(refusal) => return refusal,
    };
    let selected = match form.field("selectedGCode") {
        Ok(selected) => selected,
        Err(refusal) => return refusal,
    };
    let Ok(target) = resolve_inside(&state.paths.gcode_dir(), &[selected]) else {
        return forms::outside_library();
    };
    let parts: Vec<&str> = selected.split('/').collect();
    let mut changes = vec![setting("Computed Settings", "lastSelectedDirectory", parts[0])];
    changes.extend(parts.get(1).map(|name| setting("Maslow Settings", "openFile", name)));
    let named = changes.len() == 2;
    state.machine.request(Ask::SetSettings(changes)).await;
    if !named {
        return forms::status(500);
    }
    let free = state.free_memory;
    reply_status(detached(&state, move |loading| load_file(loading, target, None, Origin::Open, free)).await)
}

/// Written to a hidden file beside `target` and renamed over it, so no reader sees half an upload.
fn save_upload(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    static UPLOADS: AtomicU64 = AtomicU64::new(0);
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    let partial = target.with_file_name(format!(".{name}.{}.part", UPLOADS.fetch_add(1, Ordering::Relaxed)));
    std::fs::write(&partial, bytes).and_then(|()| std::fs::rename(&partial, target)).inspect_err(|_| {
        let _ = std::fs::remove_file(&partial);
    })
}

/// In order, stopping at the first that fails, as Python's loop does; off the handler, so a client leaving
/// cannot stop it part way.
async fn save_uploads(uploads: &[&forms::Upload], targets: &[PathBuf]) -> std::io::Result<()> {
    let files: Vec<(PathBuf, Bytes)> =
        uploads.iter().zip(targets).map(|(upload, target)| (target.clone(), upload.bytes.clone())).collect();
    let saved =
        tokio::task::spawn_blocking(move || files.iter().try_for_each(|(target, bytes)| save_upload(target, bytes)));
    saved.await.map_err(std::io::Error::other)?
}

pub(crate) async fn upload(State(state): State<AppState>, request: Request) -> Response {
    if let Some(refusal) = locked(&state) {
        return refusal;
    }
    let form = match Form::read(request).await {
        Ok(form) => form,
        Err(refusal) => return refusal,
    };
    let directory = match form.field("selectedDirectory") {
        Ok(directory) => directory,
        Err(refusal) => return refusal,
    };
    let root = state.paths.gcode_dir();
    let uploads = form.files("file[]");
    let targets: Result<Vec<PathBuf>, _> = std::iter::once(resolve_inside(&root, &[directory]))
        .chain(uploads.iter().map(|u| resolve_inside(&root, &[directory, &secure_filename(&u.file_name)])))
        .collect();
    let Ok(targets) = targets else {
        return forms::outside_library();
    };
    state
        .machine
        .request(Ask::SetSettings(vec![setting("Computed Settings", "lastSelectedDirectory", directory)]))
        .await;
    let Some(first) = uploads.first() else {
        return forms::status(500);
    };
    if let Err(error) = save_uploads(&uploads, &targets[1..]).await {
        tracing::warn!(%error, "could not save an upload");
        return forms::status(500);
    }
    let (text, target) = (String::from_utf8_lossy(&first.bytes).into_owned(), targets[1].clone());
    let free = state.free_memory;
    reply_status(detached(&state, move |loading| load_file(loading, target, Some(text), Origin::Open, free)).await)
}

/// `GCodeFile.saveFile`: every line followed by a newline.
async fn write_lines(target: &Path, lines: &[String]) -> std::io::Result<()> {
    let text: String = lines.iter().map(|line| format!("{line}\n")).collect();
    tokio::fs::write(target, text).await
}

pub(crate) async fn save(State(state): State<AppState>, request: Request) -> Response {
    let form = match Form::read(request).await {
        Ok(form) => form,
        Err(refusal) => return refusal,
    };
    let (name, directory) = match (form.field("fileName"), form.field("selectedDirectory")) {
        (Ok(name), Ok(directory)) => (name, directory),
        (Err(refusal), _) | (_, Err(refusal)) => return refusal,
    };
    let Ok(target) = resolve_inside(&state.paths.gcode_dir(), &[directory, name]) else {
        return forms::outside_library();
    };
    let machine = &state.machine;
    machine.request(Ask::SetSettings(vec![setting("Computed Settings", "lastSelectedDirectory", directory)])).await;
    if let Err(error) = write_lines(&target, &machine.snapshot().gcode).await {
        tracing::warn!(%error, target = %target.display(), "could not save the gcode");
        machine.request(Ask::Problem(Problem::GcodeUnsaved)).await;
        return forms::status(500);
    }
    machine.request(Ask::SetSettings(vec![setting("Maslow Settings", "openFile", name)])).await;
    forms::status(200)
}

/// `updateGCode`: Python answers success even when the text could not be loaded, after its alert.
pub(crate) async fn edit(State(state): State<AppState>, request: Request) -> Response {
    if let Some(refusal) = locked(&state) {
        return refusal;
    }
    let form = match Form::read(request).await {
        Ok(form) => form,
        Err(refusal) => return refusal,
    };
    let text = match form.field("gcode") {
        Ok(text) => text.trim_end().to_string(),
        Err(refusal) => return refusal,
    };
    let free = state.free_memory;
    let reply = detached(&state, move |loading| async move {
        let snapshot = loading.machine.snapshot();
        let edited = |gcode| Ask::LoadGcode { gcode: Box::new(gcode), file: None, origin: Origin::Edit };
        match (text.is_empty(), snapshot.gcode_file.clone()) {
            (true, Some(file)) => load_file(loading, file, None, Origin::Reload, free).await,
            (true, None) => loading.finish(edited(Gcode::empty(snapshot.units()))).await,
            (false, _) => match parse(text, snapshot.settings).await {
                Some(gcode) => loading.finish(edited(gcode)).await,
                None => loading.unreadable().await,
            },
        }
    })
    .await;
    match reply {
        Reply::Refused(error) => forms::refused(409, &error),
        _ => forms::result(true),
    }
}

pub(crate) async fn send(State(state): State<AppState>, request: Request) -> Response {
    let form = match Form::read(request).await {
        Ok(form) => form,
        Err(refusal) => return refusal,
    };
    match form.field("gcode") {
        Ok(text) => match state.machine.request(Ask::SendGcode(text.trim_end().to_string())).await {
            Reply::Refused(error) => forms::refused(409, &error),
            reply => forms::result(reply == Reply::Done),
        },
        Err(refusal) => refusal,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;
    use wc_machine::{Machine, NoHost, NoPort, spawn};
    use wc_protocol::Action;

    use super::*;

    #[tokio::test]
    async fn a_load_dropped_unfinished_ends_as_unreadable_and_frees_play() {
        let machine = Machine::new("10.0.0.5:5000", Settings::defaults());
        let (machine, _task) = spawn(machine, Box::new(NoPort), Arc::new(NoHost));
        let mut pushes = machine.subscribe();
        drop(Loading::begin(machine.clone()).await);
        machine.send(Event::Action(Action::new("startRun", json!(""), json!(""))));
        machine.request(Ask::SetSettings(Vec::new())).await;
        let alerts: Vec<String> = std::iter::from_fn(|| pushes.try_recv().ok())
            .filter(|push| push.data["command"] == "activateModal")
            .map(|push| push.data["data"].to_string())
            .collect();
        assert_eq!(alerts.len(), 2, "{alerts:?}");
        assert!(alerts[0].contains("Cannot open gcode file."));
        assert!(alerts[1].contains("No GCode file loaded."), "Play is not held: {alerts:?}");
    }
}
