//! The `main` slice: the front page's messages and `GET /api/state`.

use serde::Serialize;

use crate::Message;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostAddress {
    pub host_address: String,
}

impl Message for HostAddress {
    const COMMAND: &'static str = "hostAddress";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkStatus {
    Connected,
    Disconnected,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatus {
    pub status: LinkStatus,
    pub port: String,
    pub fake_servo_status: bool,
}

impl ConnectionStatus {
    /// The port is the sentinel `"none"` while disconnected (wart 6).
    pub fn disconnected() -> ConnectionStatus {
        ConnectionStatus { status: LinkStatus::Disconnected, port: "none".into(), fake_servo_status: false }
    }
}

impl Message for ConnectionStatus {
    const COMMAND: &'static str = "connectionStatus";
}

/// `UIProcessor.activateModal` for alerts and notifications, with Python's text flags.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivateModal {
    pub title: String,
    pub message: String,
    pub resume: String,
    pub progress: String,
    pub modal_size: String,
    pub modal_type: String,
}

impl ActivateModal {
    /// A refused or failed action (wart 4).
    pub fn alert(message: &str) -> ActivateModal {
        ActivateModal {
            title: "Alert".into(),
            message: message.into(),
            resume: "false".into(),
            progress: "false".into(),
            modal_size: "small".into(),
            modal_type: "alert".into(),
        }
    }
}

impl Message for ActivateModal {
    const COMMAND: &'static str = "activateModal";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Units {
    #[serde(rename = "MM")]
    Mm,
    #[serde(rename = "INCHES")]
    Inches,
}

impl Units {
    /// Anything but `MM` is inches, the settings file's default.
    pub fn from_setting(text: &str) -> Units {
        if text == "MM" { Units::Mm } else { Units::Inches }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PauseButton {
    Pause,
    Resume,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Python,
    Rust,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Macro {
    pub command: String,
    pub title: String,
}

impl Macro {
    pub fn new(number: u8, title: &str) -> Macro {
        Macro { command: format!("macro{number}"), title: title.into() }
    }
}

/// `GET /api/state`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupState {
    pub units: Units,
    pub dist_to_move: f64,
    pub units_z: Units,
    pub dist_to_move_z: f64,
    pub home_position: Point,
    pub pause_button_setting: PauseButton,
    pub job_in_progress: bool,
    pub manual_z_axis_adjust: bool,
    pub host_address: String,
    pub controller_firmware_version: f64,
    pub controller_board: Option<String>,
    pub firmware_build: Option<String>,
    pub macros: Vec<Macro>,
    pub backend: Backend,
}
