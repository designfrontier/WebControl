//! Backups, diagnostics and the settings imports (`Actions.backupWebControl`, `restoreWebControl`,
//! `downloadDiagnostics`, `File/importFile.py`): zips both backends can read, written and read off the async runtime.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use axum::Json;
use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use wc_config::Settings;
use wc_config::library::{keep_newest, secure_filename};
use wc_machine::files::{Problem, Reply, Request as Ask};
use wc_protocol::files::ClassicResult;
use zip::write::SimpleFileOptions;

use crate::AppState;
use crate::forms::{self, Form};

/// `KEEP_ARCHIVES`: older backups and diagnostics are deleted.
pub const KEEP_ARCHIVES: usize = 5;
/// What a diagnostics zip holds, where present: the settings and both backends' logs.
const DIAGNOSTICS: [&str; 7] =
    ["webcontrol.json", "alog.txt", "log.txt", "log.txt.1", "alog.txt.1", "webcontrol-rs.log", "webcontrol-rs.log.1"];

/// `time.strftime("%Y%m%d-%H%M%S")` in local time.
pub fn timestamp(seconds: i64) -> String {
    let time = seconds as libc::time_t;
    // SAFETY: localtime_r only writes the tm it is given, and an all-zero tm is a valid value.
    let tm = unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&time, &mut tm);
        tm
    };
    let (year, month, day) = (tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday);
    format!("{year:04}{month:02}{day:02}-{:02}{:02}{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec)
}

fn now_stamp() -> String {
    let seconds = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    timestamp(seconds as i64)
}

/// Every regular file under `dir`, relative to it, as `os.walk` finds them (symlinked directories not followed).
fn walk(dir: &Path, relative: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<_> =
        std::fs::read_dir(dir.join(relative)).into_iter().flatten().filter_map(Result::ok).collect();
    entries.sort_by_key(|entry| entry.file_name());
    entries
        .into_iter()
        .flat_map(|entry| {
            let path = relative.join(entry.file_name());
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => walk(dir, &path),
                Ok(_) if dir.join(&path).is_file() => vec![path],
                _ => Vec::new(),
            }
        })
        .collect()
}

/// A deflated zip of `files` (paths relative to `dir`) at `target`.
fn zip_files(target: &Path, dir: &Path, files: &[PathBuf]) -> zip::result::ZipResult<()> {
    let mut zip = zip::ZipWriter::new(File::create(target)?);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    files.iter().try_for_each(|file| {
        let name = file.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
        zip.start_file(name, options)?;
        let mut bytes = Vec::new();
        File::open(dir.join(file))?.read_to_end(&mut bytes)?;
        zip.write_all(&bytes).map_err(zip::result::ZipError::from)
    })?;
    zip.finish().map(drop)
}

/// `backupWebControl`: all of `~/.WebControl`, zipped into the home directory.
pub fn backup(home: &Path) -> zip::result::ZipResult<PathBuf> {
    let dir = home.join(".WebControl");
    let target = home.join(format!("wc_backup_{}.zip", now_stamp()));
    zip_files(&target, &dir, &walk(&dir, Path::new("")))?;
    keep_newest(home, "wc_backup_", ".zip", KEEP_ARCHIVES);
    Ok(target)
}

/// `downloadDiagnostics`: the settings and the logs, zipped into `~/.WebControl`.
pub fn diagnostics(dir: &Path) -> zip::result::ZipResult<PathBuf> {
    let target = dir.join(format!("wc_diagnostics_{}.zip", now_stamp()));
    let present: Vec<PathBuf> = DIAGNOSTICS.iter().map(PathBuf::from).filter(|name| dir.join(name).is_file()).collect();
    zip_files(&target, dir, &present)?;
    keep_newest(dir, "wc_diagnostics_", ".zip", KEEP_ARCHIVES);
    Ok(target)
}

/// `restoreWebControl`: a zip whose `webcontrol.json` holds settings, unpacked into `~/.WebControl`; its
/// settings, or why not. Entries that would land outside the directory are skipped.
pub fn restore(archive: &Path, dir: &Path) -> Result<Settings, String> {
    let mut zip = zip::ZipArchive::new(File::open(archive).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let mut text = String::new();
    zip.by_name("webcontrol.json")
        .map_err(|_| "Restore file has no webcontrol.json".to_string())?
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    let settings = Settings::from_text(&text).ok_or("Restore file's webcontrol.json holds no settings")?;
    (0..zip.len()).try_for_each(|index| {
        let mut entry = zip.by_index(index).map_err(|e| e.to_string())?;
        let Some(relative) = entry.enclosed_name() else {
            tracing::warn!(index, "skipping a restore entry outside the directory");
            return Ok(());
        };
        let target = dir.join(relative);
        if entry.is_dir() {
            return std::fs::create_dir_all(&target).map_err(|e| e.to_string());
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = File::create(&target).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map(drop).map_err(|e| e.to_string())
    })?;
    Ok(settings)
}

fn zip_response(path: &Path, attachment: bool) -> Response {
    match std::fs::read(path) {
        Ok(bytes) => {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let disposition = format!("{}; filename={name}", if attachment { "attachment" } else { "inline" });
            ([(header::CONTENT_TYPE, "application/zip".to_string()), (header::CONTENT_DISPOSITION, disposition)], bytes)
                .into_response()
        }
        Err(error) => failed(&error.to_string()),
    }
}

fn failed(error: &str) -> Response {
    tracing::warn!(error, "archive failed");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(ClassicResult::Failed)).into_response()
}

async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    tokio::task::spawn_blocking(work).await.ok()
}

pub(crate) async fn download_backup(State(state): State<AppState>) -> Response {
    let home = state.paths.home();
    match blocking(move || backup(&home)).await {
        Some(Ok(path)) => zip_response(&path, false),
        Some(Err(error)) => failed(&error.to_string()),
        None => failed("the backup stopped"),
    }
}

pub(crate) async fn download_diagnostics(State(state): State<AppState>) -> Response {
    let dir = state.paths.dir();
    match blocking(move || diagnostics(&dir)).await {
        Some(Ok(path)) => zip_response(&path, true),
        Some(Err(error)) => failed(&error.to_string()),
        None => failed("the diagnostics stopped"),
    }
}

/// The upload saved where Python saved it, under its secure name.
async fn saved_upload(request: Request, dir: PathBuf) -> Result<PathBuf, Response> {
    let form = Form::read(request).await?;
    let upload = form.file("file")?;
    let target = dir.join(secure_filename(&upload.file_name));
    let saved = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&target, &upload.bytes));
    saved.map(|_| target).map_err(|error| {
        tracing::warn!(%error, "could not save the upload");
        forms::status(500)
    })
}

pub(crate) async fn restore_upload(State(state): State<AppState>, request: Request) -> Response {
    let archive = match saved_upload(request, state.paths.home()).await {
        Ok(archive) => archive,
        Err(refusal) => return refusal,
    };
    let dir = state.paths.dir();
    match blocking(move || restore(&archive, &dir)).await {
        Some(Ok(settings)) => {
            state.machine.request(Ask::RestoreSettings(Box::new(settings))).await;
            forms::status(200)
        }
        Some(Err(error)) => {
            tracing::warn!(error, "restore refused");
            forms::status(500)
        }
        None => forms::status(500),
    }
}

async fn import(state: AppState, request: Request, ask: fn(String) -> Option<Ask>) -> Response {
    let file = match saved_upload(request, state.paths.imports_dir()).await {
        Ok(file) => file,
        Err(refusal) => return refusal,
    };
    let text = tokio::fs::read(&file).await.map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
    let reply = match text.ok().and_then(ask) {
        Some(request) => state.machine.request(request).await,
        None => {
            state.machine.request(Ask::Problem(Problem::ImportUnreadable)).await;
            Reply::Failed
        }
    };
    forms::status(if reply == Reply::Done { 200 } else { 500 })
}

pub(crate) async fn import_ini(State(state): State<AppState>, request: Request) -> Response {
    import(state, request, |text| Some(Ask::ImportIni(text))).await
}

pub(crate) async fn import_webcontrol(State(state): State<AppState>, request: Request) -> Response {
    import(state, request, |text| serde_json::from_str(&text).ok().map(Ask::ImportWebControl)).await
}
