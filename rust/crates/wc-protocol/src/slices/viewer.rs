//! The `viewer` slice: what the 3D view draws.

use serde::Serialize;

/// The machine's frame in inches; `None` where the setting is not a number.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineFrame {
    pub bed_width: Option<f64>,
    pub bed_height: Option<f64>,
    pub motor_offset_y: Option<f64>,
    pub motor_spacing_x: Option<f64>,
}

/// `GET /api/viewer`; the gcode and board payloads arrive with files and boards (M3), so they are `null` for now.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerSnapshot {
    pub machine: MachineFrame,
    pub gcode: Option<String>,
    pub board: Option<serde_json::Value>,
    pub cut_data: Option<String>,
}
