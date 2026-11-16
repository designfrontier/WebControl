//! The live camera overlay while optical calibration is on: `toggleCamera`, `queryCamera` and
//! `statusRequest "cameraStatus"`. The server's capture task reads [`crate::Snapshot::camera`] and reports back.

use serde::Serialize;
use serde_json::Value;
use wc_protocol::Message;

use crate::machine::{Effect, Machine, alert, broadcast};

/// What the capture task tells the machine.
#[derive(Debug, Clone, PartialEq)]
pub enum CameraReport {
    /// A preview for the overlay, base64 PNG.
    Frame(String),
    /// The camera would not open or stopped giving frames.
    Failed(String),
}

#[derive(Serialize)]
#[serde(tag = "command", content = "data")]
enum CameraMessage {
    #[serde(rename = "cameraImageUpdated")]
    Image(String),
    #[serde(rename = "updateCamera")]
    Status(&'static str),
}

impl Message for CameraMessage {
    const COMMAND: &'static str = "cameraMessage";
}

impl Machine {
    /// The overlay is on and optical calibration still is.
    pub(crate) fn camera_wanted(&self) -> bool {
        self.camera_on && crate::optical::enabled(&self.settings)
    }

    fn camera_status(&self) -> Effect {
        broadcast(&CameraMessage::Status(if self.camera_wanted() { "on" } else { "off" }))
    }

    /// No-ops while optical calibration is off, as before it existed.
    pub(crate) fn camera_action(&mut self, command: &str, arg: &Value, effects: &mut Vec<Effect>) -> Option<String> {
        if !crate::optical::enabled(&self.settings) || (command == "statusRequest" && arg != "cameraStatus") {
            return None;
        }
        if command == "toggleCamera" {
            self.camera_on = !self.camera_on;
        }
        effects.push(self.camera_status());
        None
    }

    pub(crate) fn camera_report(&mut self, report: CameraReport, effects: &mut Vec<Effect>) {
        if !self.camera_wanted() {
            return;
        }
        match report {
            CameraReport::Frame(png) => effects.push(broadcast(&CameraMessage::Image(png))),
            CameraReport::Failed(why) => {
                self.camera_on = false;
                effects.push(self.camera_status());
                effects.push(alert(&why));
            }
        }
    }
}
