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

/// The display theme every client shows, from the `uiTheme` setting; `System` follows each device's colour scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    Dark,
    System,
}

impl Theme {
    pub fn parse(text: &str) -> Option<Theme> {
        match text {
            "light" => Some(Theme::Light),
            "dark" => Some(Theme::Dark),
            "system" => Some(Theme::System),
            _ => None,
        }
    }

    /// Anything but `light` or `dark` reads as `System`.
    pub fn from_setting(text: &str) -> Theme {
        Theme::parse(text).unwrap_or(Theme::System)
    }

    pub fn name(self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
            Theme::System => "system",
        }
    }
}

/// Broadcast when the display theme setting changes, so every open client switches; also
/// `PUT /api/display/theme`'s body and answer.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ThemeChanged {
    pub theme: Theme,
}

impl Message for ThemeChanged {
    const COMMAND: &'static str = "theme";
}
