//! The `boards` slice: the current board and the board library.

use serde::Serialize;
use serde_json::Value;

use crate::Message;

/// `boardDataUpdate`: the board file's info line, dimensions as the file holds them (wart 5).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct BoardDataUpdate(pub Value);

impl Message for BoardDataUpdate {
    const COMMAND: &'static str = "boardDataUpdate";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BoardUnits {
    Mm,
    Inches,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Router {
    pub x: f64,
    pub y: f64,
}

/// `GET /api/boards/current`: dimensions in the machine's units, and the router for aligning.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentBoard {
    #[serde(rename = "boardID")]
    pub board_id: String,
    pub material: String,
    pub width: f64,
    pub height: f64,
    pub thickness: f64,
    pub center_x: f64,
    pub center_y: f64,
    pub units: BoardUnits,
    pub file_name: String,
    pub router: Router,
}
