//! The `files` slice: the gcode library, the loaded gcode, and the classic form routes' answers.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Message;

/// Broadcast after a directory is created in the gcode library.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpdateDirectories {
    pub directory: String,
}

impl Message for UpdateDirectories {
    const COMMAND: &'static str = "updateDirectories";
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LibraryFile {
    pub directory: String,
    pub name: String,
}

/// `GET /api/files/gcode` and, without `valid_extensions`, `GET /api/boards/files`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    pub directories: Vec<String>,
    pub files: Vec<LibraryFile>,
    pub last_directory: String,
    pub last_file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_extensions: Option<String>,
}

/// `POST /api/files/gcode/directories` and `/api/boards/directories`: the sanitised name and the new listing.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CreatedDirectory {
    pub directory: String,
    pub directories: Vec<String>,
    pub files: Vec<LibraryFile>,
}

/// `POST /api/files/gcode/delete`: the listing after the delete.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LibraryListing {
    pub directories: Vec<String>,
    pub files: Vec<LibraryFile>,
}

/// `POST /api/files/gcode/delete` body: a file's library path, or a directory's name.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GcodeDelete {
    pub path: String,
}

/// `GET /api/files/gcode/loaded`: `directory` is `None` when the file is outside the library.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedGcode {
    pub directory: Option<String>,
    pub name: Option<String>,
    pub line_count: usize,
}

/// `GET /api/files/gcode/text` and `/api/files/gcode/custom`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GcodeText {
    pub text: String,
}

/// An `/api` refusal.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ApiError {
    pub error: String,
}

/// The body of a directory request: `name` as sent, which Python passes through `str()`.
#[derive(Debug, Clone, PartialEq)]
pub struct DirectoryRequest {
    pub name: String,
}

impl DirectoryRequest {
    /// `request.get_json(silent=True) or {}`: anything that is not a JSON object names nothing.
    pub fn from_body(body: &[u8]) -> DirectoryRequest {
        let name = match serde_json::from_slice::<Value>(body).ok().and_then(|v| v.get("name").cloned()) {
            Some(Value::String(text)) => text,
            Some(Value::Bool(flag)) => if flag { "True" } else { "False" }.into(),
            Some(Value::Null) => "None".into(),
            Some(other) => other.to_string(),
            None => String::new(),
        };
        DirectoryRequest { name }
    }
}

/// What most classic form routes answer, the HTTP status repeated in the body (wart 8); `error` on a refusal, and
/// `warning` on a gcode load that went ahead but may strain the host's memory.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClassicStatus {
    pub status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

impl ClassicStatus {
    pub fn new(status: u16) -> ClassicStatus {
        ClassicStatus { status, error: None, warning: None }
    }

    pub fn refused(status: u16, error: &str) -> ClassicStatus {
        ClassicStatus { status, error: Some(error.into()), warning: None }
    }

    pub fn warned(status: u16, warning: &str) -> ClassicStatus {
        ClassicStatus { status, error: None, warning: Some(warning.into()) }
    }
}

/// `/editGCode`, `/sendGCode`, `/editBoard` and `/trimBoard` answer a bare JSON string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ClassicResult {
    Success,
    Failed,
}
