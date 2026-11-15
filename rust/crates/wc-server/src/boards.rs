//! The board library and the current board: `api/boards.py` and the classic board form routes in `main.py`.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use wc_config::library::{directory_setting, list_library, resolve_inside};
use wc_gcode::board::{Board, Dim};
use wc_gcode::py;
use wc_machine::files::{BoardForm, Problem, Reply, Request as Ask, TrimForm};
use wc_protocol::boards::{BoardUnits, CurrentBoard, Router};
use wc_protocol::files::Library;
use wc_protocol::main::Units;

use crate::AppState;
use crate::files::{create_directory, files};
use crate::forms::{self, Form};

const MM_PER_INCH: f64 = 25.4;

pub(crate) async fn library(State(state): State<AppState>) -> Json<Library> {
    let snapshot = state.machine.snapshot();
    let settings = &snapshot.settings;
    let (directories, files) = files(list_library(&state.paths.boards_dir()));
    let last_file = match snapshot.board.file_name.as_str() {
        "" => settings.text_or_empty("Maslow Settings", "openBoardFile"),
        name => name.to_string(),
    };
    Json(Library {
        directories,
        files,
        last_directory: directory_setting(&settings.text_or_empty("Computed Settings", "lastSelectedBoardDirectory")),
        last_file,
        valid_extensions: None,
    })
}

pub(crate) async fn new_directory(State(state): State<AppState>, body: Bytes) -> Response {
    create_directory(&state.paths.boards_dir(), &body).map_or_else(|refusal| refusal, |(_, response)| response)
}

/// `float()` of a stored dimension, which may be numeric text (wart 5).
fn float(dim: &Dim) -> Option<f64> {
    match dim {
        Dim::Number(number) => number.as_f64(),
        Dim::Text(text) => py::float(text),
    }
}

/// `current_board`: boards are stored in inches, as the Edit Board page converted them.
pub fn current(board: &Board, units: Units, sled: (f64, f64)) -> Option<CurrentBoard> {
    let metric = units == Units::Mm;
    let scale = if metric { MM_PER_INCH } else { 1.0 };
    let scaled = |dim: &Dim| float(dim).map(|value| py::round(value * scale, 2));
    Some(CurrentBoard {
        board_id: board.board_id.clone(),
        material: board.material.clone(),
        width: scaled(&board.width)?,
        height: scaled(&board.height)?,
        thickness: scaled(&board.thickness)?,
        center_x: scaled(&board.center_x)?,
        center_y: scaled(&board.center_y)?,
        units: if metric { BoardUnits::Mm } else { BoardUnits::Inches },
        file_name: board.file_name.clone(),
        router: Router { x: sled.0, y: sled.1 },
    })
}

pub(crate) async fn current_board(State(state): State<AppState>) -> Response {
    let snapshot = state.machine.snapshot();
    match current(&snapshot.board, snapshot.units(), snapshot.sled) {
        Some(board) => Json(board).into_response(),
        None => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "The board is not a number"})))
            .into_response(),
    }
}

fn setting(section: &str, key: &str, value: &str) -> (String, String, serde_json::Value) {
    (section.into(), key.into(), value.into())
}

pub(crate) async fn open(State(state): State<AppState>, request: Request) -> Response {
    let form = match Form::read(request).await {
        Ok(form) => form,
        Err(refusal) => return refusal,
    };
    let selected = match form.field("selectedBoard") {
        Ok(selected) => selected,
        Err(refusal) => return refusal,
    };
    let Ok(target) = resolve_inside(&state.paths.boards_dir(), &[selected]) else {
        return forms::outside_library();
    };
    let parts: Vec<&str> = selected.split('/').collect();
    let changes = vec![
        setting("Computed Settings", "lastSelectedBoardDirectory", parts[0]),
        setting("Maslow Settings", "openBoardFile", parts[parts.len() - 1]),
    ];
    state.machine.request(Ask::SetSettings(changes)).await;
    let reply = match tokio::fs::read(&target).await {
        Ok(bytes) => state.machine.request(Ask::LoadBoard(String::from_utf8_lossy(&bytes).into_owned())).await,
        Err(error) => {
            tracing::warn!(%error, target = %target.display(), "could not read the board");
            state.machine.request(Ask::Problem(Problem::BoardUnreadable)).await;
            Reply::Failed
        }
    };
    forms::status(if reply == Reply::Done { 200 } else { 500 })
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
    let Ok(target) = resolve_inside(&state.paths.boards_dir(), &[directory, name]) else {
        return forms::outside_library();
    };
    let machine = &state.machine;
    machine
        .request(Ask::SetSettings(vec![setting("Computed Settings", "lastSelectedBoardDirectory", directory)]))
        .await;
    let file_name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let saved = match machine.request(Ask::SaveBoard(file_name)).await {
        Reply::Text(text) => match tokio::fs::write(&target, text).await {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(%error, target = %target.display(), "could not save the board");
                machine.request(Ask::Problem(Problem::BoardUnsaved)).await;
                false
            }
        },
        _ => false,
    };
    machine.request(Ask::SetSettings(vec![setting("Maslow Settings", "openBoardFile", name)])).await;
    forms::status(if saved { 200 } else { 500 })
}

/// `editBoard` reads the fields inside its `try`, so a missing one fails rather than giving Werkzeug's 400.
pub(crate) async fn edit(State(state): State<AppState>, request: Request) -> Response {
    let form = match Form::read(request).await {
        Ok(form) => form,
        Err(refusal) => return refusal,
    };
    let field = |name: &str| form.field(name).map(String::from).ok();
    let read = || -> Option<BoardForm> {
        Some(BoardForm {
            board_id: field("boardID")?,
            material: field("material")?,
            height: field("height")?,
            width: field("width")?,
            thickness: field("thickness")?,
            center_x: field("centerX")?,
            center_y: field("centerY")?,
            units: field("units")?,
        })
    };
    let reply = match read() {
        Some(edit) => state.machine.request(Ask::EditBoard(edit)).await,
        None => Reply::Failed,
    };
    forms::result(reply == Reply::Done)
}

pub(crate) async fn trim(State(state): State<AppState>, request: Request) -> Response {
    let form = match Form::read(request).await {
        Ok(form) => form,
        Err(refusal) => return refusal,
    };
    let names = ["trimTop", "trimBottom", "trimLeft", "trimRight", "units"];
    let mut values = match names.iter().map(|name| form.field(name).map(String::from)).collect::<Result<Vec<_>, _>>() {
        Ok(values) => values.into_iter(),
        Err(refusal) => return refusal,
    };
    let mut next = || values.next().unwrap_or_default();
    let trim = TrimForm { top: next(), bottom: next(), left: next(), right: next(), units: next() };
    let reply = state.machine.request(Ask::TrimBoard(trim)).await;
    forms::result(reply == Reply::Done)
}
