use serde_json::Value;

use crate::Frame;

/// A `processAction` command with its arguments as sent: text or numbers (wart 13).
#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub command: String,
    pub arg: Value,
    pub arg1: Value,
}

/// Python's `float()` of an action argument: a number or numeric text; never NaN or infinite.
pub fn float(value: &Value) -> Option<f64> {
    let number = match value {
        Value::Number(n) => n.as_f64(),
        Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    };
    number.filter(|n| n.is_finite())
}

impl Action {
    pub fn new(command: &str, arg: Value, arg1: Value) -> Action {
        Action { command: command.into(), arg, arg1 }
    }

    /// Stop goes on the machine's priority channel, never queued behind streaming.
    pub fn is_realtime(&self) -> bool {
        matches!(self.command.as_str(), "stopRun" | "stopZ")
    }

    pub fn arg_text(&self) -> String {
        self.arg.as_str().map(String::from).unwrap_or_else(|| self.arg.to_string())
    }
}

/// A client -> server event on `/MaslowCNC`.
#[derive(Debug, Clone, PartialEq)]
pub enum ClientEvent {
    Action(Action),
    SettingRequest {
        section: String,
        setting: String,
    },
    /// A contract event this backend does not decode yet.
    Unhandled(String),
}

const LATER_EVENTS: [&str; 4] = ["updateSetting", "checkForGCodeUpdate", "checkForBoardUpdate", "opticalCalibration"];

impl ClientEvent {
    /// `None` for an unknown event or a malformed payload; the transport ignores those.
    pub fn from_frame(frame: &Frame) -> Option<ClientEvent> {
        match frame.event.as_str() {
            "action" => {
                let data = frame.data.get("data")?;
                let arg = |name: &str| data.get(name).cloned().unwrap_or_else(|| Value::from(""));
                let command = data.get("command")?.as_str()?.to_string();
                Some(ClientEvent::Action(Action { command, arg: arg("arg"), arg1: arg("arg1") }))
            }
            "settingRequest" => {
                let data = frame.data.get("data")?;
                let text = |name: &str| data.get(name)?.as_str().map(String::from);
                Some(ClientEvent::SettingRequest { section: text("section")?, setting: text("setting")? })
            }
            event if LATER_EVENTS.contains(&event) => Some(ClientEvent::Unhandled(event.into())),
            _ => None,
        }
    }
}
