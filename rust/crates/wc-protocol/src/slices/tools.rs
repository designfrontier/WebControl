//! The `tools` slice: the user's tool library and the tools a loaded file names in its CAM comments.

use serde::{Deserialize, Serialize};

use crate::Message;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolUnits {
    Mm,
    In,
}

/// One library entry, as stored in `tools.json` and sent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tool {
    pub number: u32,
    pub description: String,
    pub diameter: Option<f64>,
    pub units: Option<ToolUnits>,
    pub flutes: Option<u32>,
    #[serde(default)]
    pub notes: String,
}

/// `GET /api/tools`, every write's answer and the `toolLibrary` broadcast.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ToolLibrary {
    pub tools: Vec<Tool>,
}

impl Message for ToolLibrary {
    const COMMAND: &'static str = "toolLibrary";
}

/// A tool the loaded file uses or describes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FileTool {
    pub number: u32,
    pub description: Option<String>,
    pub diameter: Option<f64>,
    pub units: Option<ToolUnits>,
    pub flutes: Option<u32>,
}

/// `GET /api/files/gcode/tools`.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct FileTools {
    pub tools: Vec<FileTool>,
}
