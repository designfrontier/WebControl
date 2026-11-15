use std::time::Duration;

use serde_json::{Map, Value, json};
use wc_protocol::main::{ActivateModal, ConnectionStatus, HostAddress, LinkStatus, PauseButton};
use wc_protocol::{Action, Envelope, Push};

/// Everything that can happen to the machine.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A UI action from the command channel.
    Action(Action),
    /// `stopRun`/`stopZ` from the realtime channel.
    Stop(Action),
    ClientConnected {
        sid: String,
    },
    ControllerLine(String),
    Tick,
}

/// What the shell must do after an event, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Bytes through the controller's line buffer.
    Write(Vec<u8>),
    /// `!` or `~`, which the firmware acts on at once.
    Realtime(Vec<u8>),
    Push(Push),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunState {
    Idle,
}

/// What readers see; every field is derived from the machine's state, never stored separately.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub link: LinkStatus,
    pub upload_flag: i32,
    pub job_in_progress: bool,
    pub manual_z_axis_adjust: bool,
    pub pause_button: PauseButton,
    pub controller_firmware_version: f64,
    pub controller_board: Option<String>,
    pub firmware_build: Option<String>,
    pub host_address: String,
}

impl Snapshot {
    /// The `GET /api/state` / `statusMessage` names a transcript's `expect.state` checks.
    pub fn state_fields(&self) -> Map<String, Value> {
        let fields = json!({
            "uploadFlag": self.upload_flag,
            "jobInProgress": self.job_in_progress,
            "manualZAxisAdjust": self.manual_z_axis_adjust,
            "pauseButtonSetting": self.pause_button,
            "controllerFirmwareVersion": self.controller_firmware_version,
            "controllerBoard": self.controller_board,
            "firmwareBuild": self.firmware_build,
        });
        fields.as_object().cloned().unwrap_or_default()
    }
}

pub struct Machine {
    host_address: String,
    link: LinkStatus,
    run: RunState,
}

fn broadcast(envelope: Envelope) -> Effect {
    Effect::Push(Push::message(envelope))
}

impl Machine {
    pub fn new(host_address: &str) -> Machine {
        Machine { host_address: host_address.into(), link: LinkStatus::Disconnected, run: RunState::Idle }
    }

    pub fn handle(&mut self, event: Event, _now: Duration) -> Vec<Effect> {
        match event {
            Event::ClientConnected { .. } => vec![
                broadcast(Envelope::json(&HostAddress { host_address: self.host_address.clone() })),
                broadcast(Envelope::json(&ConnectionStatus::disconnected())),
            ],
            // statusRequest only asks for the camera's status, and the camera is out of scope.
            Event::Action(action) if action.command == "statusRequest" => Vec::new(),
            Event::Action(action) => {
                let text = format!("Function not currently implemented.. Sorry.[{}]", action.command);
                vec![broadcast(Envelope::json(&ActivateModal::alert(&text)))]
            }
            Event::Stop(_) | Event::ControllerLine(_) | Event::Tick => Vec::new(),
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        let RunState::Idle = self.run;
        Snapshot {
            link: self.link,
            upload_flag: 0,
            job_in_progress: false,
            manual_z_axis_adjust: false,
            pause_button: PauseButton::Pause,
            controller_firmware_version: 0.0,
            controller_board: None,
            firmware_build: None,
            host_address: self.host_address.clone(),
        }
    }
}
