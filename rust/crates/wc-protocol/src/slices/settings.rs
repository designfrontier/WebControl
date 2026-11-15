//! The `settings` slice: the settings screens and the serial port list.

use serde::Serialize;
use serde_json::Value;

use crate::Message;

/// Serial ports from the last `updatePorts` action; `["None"]` when none was found (wart 6).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct UpdatePorts(pub Vec<String>);

impl Message for UpdatePorts {
    const COMMAND: &'static str = "updatePorts";
}

/// One setting as a form field (`api/settings.py` `describe_setting`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SettingField {
    pub key: String,
    pub title: String,
    pub desc: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub value: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<String>>,
    pub custom: bool,
    pub numeric: bool,
}

/// `GET /api/settings/{sectionId}`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSection {
    pub id: String,
    pub title: String,
    pub enable_custom: bool,
    pub settings: Vec<SettingField>,
    /// Only the Maslow section lists the ports.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ports: Option<Vec<String>>,
}
