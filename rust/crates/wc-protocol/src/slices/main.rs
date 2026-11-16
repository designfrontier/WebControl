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
        ActivateModal::titled_alert("Alert", message)
    }

    pub fn titled_alert(title: &str, message: &str) -> ActivateModal {
        ActivateModal {
            title: title.into(),
            message: message.into(),
            resume: "false".into(),
            progress: "false".into(),
            modal_size: "small".into(),
            modal_type: "alert".into(),
        }
    }

    /// A controller message; `resume` offers the Resume button.
    pub fn notification(message: &str, resume: bool) -> ActivateModal {
        ActivateModal {
            title: "Notification:".into(),
            resume: if resume { "resume" } else { "false" }.into(),
            modal_type: "notification".into(),
            ..ActivateModal::alert(message)
        }
    }
}

/// The sled position from a status report, sent when it moved.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PositionMessage {
    pub xval: f64,
    pub yval: f64,
    pub zval: f64,
    pub vel: f64,
    pub rqvel: f64,
    /// Percent of the file sent, as text like `"12.3%"` (wart 6).
    pub pcom: String,
    pub state: String,
    pub lpwr: f64,
    pub rpwr: f64,
    pub zpwr: f64,
}

impl Message for PositionMessage {
    const COMMAND: &'static str = "positionMessage";
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorValueMessage {
    pub left_error: f64,
    pub right_error: f64,
    /// `-999999` when there is no forward-kinematics position (wart 6).
    #[serde(rename = "computedX")]
    pub computed_x: f64,
    #[serde(rename = "computedY")]
    pub computed_y: f64,
    pub computed_enabled: i64,
}

impl Message for ErrorValueMessage {
    const COMMAND: &'static str = "errorValueMessage";
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthMessage {
    pub cpu_usage: f64,
    /// `-1` when buffering is off (wart 6).
    pub buffer_size: i64,
    pub controller_firmware_version: f64,
}

impl Message for HealthMessage {
    const COMMAND: &'static str = "healthMessage";
}

/// Also the `status` of `GET /api/main`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusMessage {
    pub upload_flag: i32,
    pub positioning_mode: u8,
    pub current_tool: u32,
    pub job_in_progress: bool,
    #[serde(rename = "manualZAxisAdjust")]
    pub manual_z_axis_adjust: bool,
    /// A G38.2 probe is queued or descending (Rust only).
    pub probing: bool,
}

impl Message for StatusMessage {
    const COMMAND: &'static str = "statusMessage";
}

/// A controller line WebControl does not act on, for the Controller Messages pane.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ControllerMessage(pub String);

impl Message for ControllerMessage {
    const COMMAND: &'static str = "controllerMessage";
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Alarm {
    pub message: String,
}

impl Message for Alarm {
    const COMMAND: &'static str = "alarm";
}

/// Always `{"data": ""}`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClearAlarm {
    pub data: String,
}

impl ClearAlarm {
    pub fn new() -> ClearAlarm {
        ClearAlarm { data: String::new() }
    }
}

impl Default for ClearAlarm {
    fn default() -> Self {
        ClearAlarm::new()
    }
}

impl Message for ClearAlarm {
    const COMMAND: &'static str = "clearAlarm";
}

/// Rust always sends numbers here (wart 5).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HomePositionMessage {
    pub xval: f64,
    pub yval: f64,
}

impl Message for HomePositionMessage {
    const COMMAND: &'static str = "homePositionMessage";
}

/// The answer to `settingRequest`, broadcast (wart 12); `value` as the settings file stores it (wart 5).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RequestedSetting {
    pub setting: String,
    pub value: serde_json::Value,
}

impl Message for RequestedSetting {
    const COMMAND: &'static str = "requestedSetting";
}

/// The file's position at the current line, for the viewer's marker; in the machine's units.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GcodePositionMessage {
    pub xval: f64,
    pub yval: f64,
    pub zval: f64,
    pub gcode_line: String,
    pub gcode_line_index: usize,
}

impl Message for GcodePositionMessage {
    const COMMAND: &'static str = "gcodePositionMessage";
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GcodeLine {
    pub line_index: usize,
    pub line: String,
    pub line_count: usize,
}

/// `GET /api/main`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MainScreen {
    pub status: StatusMessage,
    pub touch_plate: bool,
    pub gcode: GcodeLine,
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
    /// A G38.2 probe is queued or descending (Rust only).
    pub probing: bool,
    pub host_address: String,
    pub controller_firmware_version: f64,
    pub controller_board: Option<String>,
    pub firmware_build: Option<String>,
    pub macros: Vec<Macro>,
    pub backend: Backend,
    /// The webcontrol-rs release (Rust only), which `webcontrol-update` checks after a restart.
    pub version: String,
    /// The git commit the binary was built from (Rust only), shown beside the release version.
    pub commit: String,
    /// The experimental features turned on in the settings, such as `"optical"` (Rust only).
    pub experimental: Vec<String>,
}
