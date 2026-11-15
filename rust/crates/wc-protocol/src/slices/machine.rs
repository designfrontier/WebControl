//! The `machine` slice: the setup screens, quick configure and the PID results.

use serde::Serialize;
use serde_json::{Map, Value};

use crate::Message;

/// `GET /api/machine/setup`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineSetup {
    pub chain_extend_length: f64,
    pub four_motor: bool,
    pub firmware_supports_zaxis_limit: bool,
    pub touch_plate: bool,
    pub controller_firmware_version: f64,
    pub firmware_build: Option<String>,
}

pub const KINEMATICS_TYPES: [&str; 6] = ["Ring", "Linkage", "Pantograph", "Metal Maslow", "Custom", "Quadrilateral"];
pub const CHAIN_SIDES: [&str; 2] = ["Top", "Bottom"];

/// `GET`/`POST /api/machine/quick-configure`'s answer; lengths are mm.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickConfigure {
    pub kinematics_type: String,
    pub rotation_radius: f64,
    pub chain_over_sprocket: String,
    pub motor_spacing_x: f64,
    pub motor_offset_y: f64,
}

/// A valid `POST /api/machine/quick-configure` body.
#[derive(Debug, Clone, PartialEq)]
pub struct QuickConfigureForm {
    pub kinematics_type: String,
    pub chain_over_sprocket: String,
    pub rotation_radius: f64,
    pub motor_spacing_x: f64,
    pub motor_offset_y: f64,
}

impl QuickConfigureForm {
    /// `api/machine.py`'s `parse_quick_configure`, with its messages.
    pub fn parse(body: &Value) -> Result<QuickConfigureForm, String> {
        let body = body.as_object().ok_or("Expected a JSON object")?;
        let text = |key: &str| body.get(key).and_then(Value::as_str).map(String::from);
        let kinematics_type = text("kinematicsType")
            .filter(|t| KINEMATICS_TYPES.contains(&t.as_str()))
            .ok_or_else(|| format!("kinematicsType must be one of {}", KINEMATICS_TYPES.join(", ")))?;
        let chain_over_sprocket = text("chainOverSprocket")
            .filter(|t| CHAIN_SIDES.contains(&t.as_str()))
            .ok_or("chainOverSprocket must be Top or Bottom")?;
        let length = |key: &str| {
            body.get(key)
                .filter(|v| v.is_number())
                .and_then(Value::as_f64)
                .filter(|n| n.is_finite() && *n >= 0.0)
                .ok_or_else(|| format!("{key} must be a number of mm, zero or more"))
        };
        Ok(QuickConfigureForm {
            kinematics_type,
            chain_over_sprocket,
            rotation_radius: length("rotationRadius")?,
            motor_spacing_x: length("motorSpacingX")?,
            motor_offset_y: length("motorOffsetY")?,
        })
    }
}

/// A `PUT /api/settings/{sectionId}` body.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsUpdate {
    pub values: Map<String, Value>,
}

impl SettingsUpdate {
    pub const MALFORMED: &'static str = "Expected {\"values\": {key: value}}";

    pub fn parse(body: &Value) -> Result<SettingsUpdate, String> {
        let values = body.get("values").and_then(Value::as_object).ok_or(SettingsUpdate::MALFORMED)?;
        Ok(SettingsUpdate { values: values.clone() })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PidKind {
    Velocity,
    Position,
}

/// What a PID test reported: numbers for version 1, text lines for version 2.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PidResult {
    pub result: PidKind,
    /// As the request typed it; `0` before any test was requested.
    pub version: Value,
    pub data: Vec<Value>,
}

/// Rust sends `data` as the object, which the schema and the UI accept (plan §3.3, wart 2).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpdatePidData {
    pub command: String,
    pub data: PidResult,
}

impl UpdatePidData {
    pub fn new(data: PidResult) -> UpdatePidData {
        UpdatePidData { command: UpdatePidData::COMMAND.into(), data }
    }
}

impl Message for UpdatePidData {
    const COMMAND: &'static str = "updatePIDData";
}
