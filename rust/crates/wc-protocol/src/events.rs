use serde_json::Value;

use crate::Frame;

/// A `processAction` command with its arguments as sent: text or numbers (wart 13).
#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub command: String,
    pub arg: Value,
    pub arg1: Value,
}

impl Action {
    /// Stop goes on the machine's priority channel, never queued behind streaming.
    pub fn is_realtime(&self) -> bool {
        matches!(self.command.as_str(), "stopRun" | "stopZ")
    }
}

/// A client -> server event on `/MaslowCNC`.
#[derive(Debug, Clone, PartialEq)]
pub enum ClientEvent {
    Action(Action),
    /// A contract event this backend does not decode yet.
    Unhandled(String),
}

const LATER_EVENTS: [&str; 5] =
    ["settingRequest", "updateSetting", "checkForGCodeUpdate", "checkForBoardUpdate", "opticalCalibration"];

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
            event if LATER_EVENTS.contains(&event) => Some(ClientEvent::Unhandled(event.into())),
            _ => None,
        }
    }
}
