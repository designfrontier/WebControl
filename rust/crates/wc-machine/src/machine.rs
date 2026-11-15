use std::collections::VecDeque;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Map, Value, json};
use wc_config::Settings;
use wc_firmware::Accounting;
use wc_gcode::Positioning;
use wc_gcode::restart::positioning_word;
use wc_protocol::main::{ActivateModal, ConnectionStatus, HostAddress, LinkStatus, PauseButton, StatusMessage, Units};
use wc_protocol::{Action, Envelope, Message, Push};

use crate::ui::{ErrorTracker, PositionTracker};

/// Everything that can happen to the machine.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A UI action from the command channel.
    Action(Action),
    /// `stopRun`/`stopZ` from the realtime channel.
    Stop(Action),
    SettingRequest {
        section: String,
        setting: String,
    },
    ClientConnected {
        sid: String,
    },
    ControllerLine(String),
    /// The clock, every 10 ms, when no controller line is waiting.
    Tick,
    PortOpened {
        port: String,
    },
    PortFailed {
        port: String,
        error: String,
    },
    /// The port's reader hit the end or an error.
    PortClosed,
    /// The serial ports found for `updatePorts`.
    Ports(Vec<String>),
    /// Every 5 s, with the busiest core's load.
    Health {
        cpu: f64,
    },
}

/// What the shell must do after an event, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Bytes through the controller's line buffer.
    Write(Vec<u8>),
    /// `!` or `~`, which the firmware acts on at once.
    Realtime(Vec<u8>),
    Push(Push),
    OpenPort(String),
    ClosePort,
    ListPorts,
    /// A line for the log viewer: what was sent or read.
    Log(String),
    ClearLogs,
    PowerOff,
    Exit,
}

/// An `Arc` whose equality is identity, so comparing snapshots never walks the settings.
#[derive(Debug, Clone)]
pub struct Shared<T>(pub Arc<T>);

impl<T> PartialEq for Shared<T> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl<T> Deref for Shared<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunState {
    Idle,
    Running,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Link {
    Disconnected { retry_at: Duration },
    Opening,
    Connected { port: String },
}

/// A command waiting for the controller to go idle; the boot's own lines survive a Stop.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Queued {
    Line {
        text: String,
        boot: bool,
    },
    /// Expanded once B05 has answered, so the push knows the firmware version.
    SettingsPush,
}

/// What readers see; every field is derived from the machine's state, never stored separately.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub link: LinkStatus,
    pub port: String,
    pub upload_flag: i32,
    pub job_in_progress: bool,
    pub manual_z_axis_adjust: bool,
    pub pause_button: PauseButton,
    pub controller_firmware_version: f64,
    pub controller_board: Option<String>,
    pub firmware_build: Option<String>,
    pub host_address: String,
    pub positioning: Positioning,
    pub current_tool: u32,
    pub gcode_index: usize,
    pub gcode_line: String,
    pub gcode_count: usize,
    pub ports: Vec<String>,
    pub settings: Shared<Settings>,
}

impl Snapshot {
    pub fn status(&self) -> StatusMessage {
        StatusMessage {
            upload_flag: self.upload_flag,
            positioning_mode: u8::from(self.positioning == Positioning::Relative),
            current_tool: self.current_tool,
            job_in_progress: self.job_in_progress,
            manual_z_axis_adjust: self.manual_z_axis_adjust,
        }
    }

    pub fn units(&self) -> Units {
        Units::from_setting(&self.settings.text_or_empty("Computed Settings", "units"))
    }

    /// The `GET /api/state` / `statusMessage` names a transcript's `expect.state` checks.
    pub fn state_fields(&self) -> Map<String, Value> {
        let fields = json!({
            "uploadFlag": self.upload_flag,
            "jobInProgress": self.job_in_progress,
            "gcodeIndex": self.gcode_index,
            "units": self.units(),
            "manualZAxisAdjust": self.manual_z_axis_adjust,
            "pauseButtonSetting": self.pause_button,
            "currentTool": self.current_tool,
            "positioningMode": positioning_word(self.positioning).trim(),
            "controllerFirmwareVersion": self.controller_firmware_version,
            "controllerBoard": self.controller_board,
            "firmwareBuild": self.firmware_build,
            "home": {
                "x": self.settings.number("Advanced Settings", "homeX"),
                "y": self.settings.number("Advanced Settings", "homeY"),
            },
        });
        fields.as_object().cloned().unwrap_or_default()
    }
}

/// The controller as the link knows it; reset whenever the port opens or the board boots.
#[derive(Debug, Clone)]
pub(crate) struct Controller {
    pub accounting: Accounting,
    /// The last line was acknowledged (Python's `machineIsReadyForData`).
    pub ready: bool,
    pub last_write: Option<Duration>,
    pub last_line: Duration,
    pub boot_deadline: Option<Duration>,
    pub boot_in_progress: bool,
    pub last_banner: Option<Duration>,
    /// After `!`, an ALARM or an overflow: acks are ignored until a status report read once input ran dry.
    pub resync_after: Option<Duration>,
    pub caught_up: bool,
    pub queue: VecDeque<Queued>,
    /// The G38.2 line awaiting its `ok` or `error:`.
    pub probe: Option<u64>,
    pub eeprom_reset: bool,
}

impl Controller {
    pub fn new(now: Duration) -> Controller {
        Controller {
            accounting: Accounting::default(),
            ready: false,
            last_write: None,
            last_line: now,
            boot_deadline: None,
            boot_in_progress: false,
            last_banner: None,
            resync_after: None,
            caught_up: false,
            queue: VecDeque::new(),
            probe: None,
            eeprom_reset: false,
        }
    }
}

pub struct Machine {
    pub(crate) host_address: String,
    /// Opened in place of the `COMport` setting: the simulator's port.
    pub(crate) fixed_port: Option<String>,
    pub(crate) settings: Arc<Settings>,
    pub(crate) link: Link,
    pub(crate) controller: Controller,
    pub(crate) firmware_version: f64,
    pub(crate) rewrite_notice: bool,
    pub(crate) board: Option<String>,
    pub(crate) fake_servo: bool,
    pub(crate) positioning: Positioning,
    pub(crate) tool: u32,
    /// A Touch Z is queued or descending.
    pub(crate) probing: bool,
    pub(crate) run: RunState,
    pub(crate) gcode: Vec<String>,
    pub(crate) index: usize,
    pub(crate) ports: Vec<String>,
    pub(crate) position: PositionTracker,
    pub(crate) errors: ErrorTracker,
    pub(crate) reported_buffer: i64,
    pub(crate) last_status: Option<StatusMessage>,
}

pub(crate) fn broadcast<M: Message>(message: &M) -> Effect {
    Effect::Push(Push::message(Envelope::json(message)))
}

pub(crate) fn alert(text: &str) -> Effect {
    broadcast(&ActivateModal::alert(text))
}

impl Machine {
    pub fn new(host_address: &str, settings: Settings) -> Machine {
        Machine {
            host_address: host_address.into(),
            fixed_port: None,
            settings: Arc::new(settings),
            link: Link::Disconnected { retry_at: Duration::ZERO },
            controller: Controller::new(Duration::ZERO),
            firmware_version: 0.0,
            rewrite_notice: false,
            board: None,
            fake_servo: false,
            positioning: Positioning::Absolute,
            tool: 0,
            probing: false,
            run: RunState::Idle,
            gcode: Vec::new(),
            index: 0,
            ports: Vec::new(),
            position: PositionTracker::default(),
            errors: ErrorTracker::default(),
            reported_buffer: 127,
            last_status: None,
        }
    }

    /// Always opens `port`, whatever `COMport` says.
    pub fn on_port(self, port: &str) -> Machine {
        Machine { fixed_port: Some(port.into()), ..self }
    }

    /// Loads a file as `/openGCode` would; M3 adds the route, transcripts and tests use it directly.
    pub fn load_gcode(&mut self, text: &str) {
        self.gcode = wc_gcode::load::filter(text, None).unwrap_or_default();
        self.index = 0;
    }

    pub fn handle(&mut self, event: Event, now: Duration) -> Vec<Effect> {
        let mut effects = Vec::new();
        match event {
            Event::ClientConnected { .. } => {
                effects.push(broadcast(&HostAddress { host_address: self.host_address.clone() }));
                effects.push(broadcast(&self.connection_status()));
            }
            Event::Action(action) | Event::Stop(action) => self.action(&action, now, &mut effects),
            Event::SettingRequest { section, setting } => self.setting_request(&section, &setting, &mut effects),
            Event::ControllerLine(line) => self.controller_line(&line, now, &mut effects),
            Event::Tick => self.tick(now, &mut effects),
            Event::PortOpened { port } => self.port_opened(port, now, &mut effects),
            Event::PortFailed { port, error } => {
                tracing::debug!(port, error, "could not open the controller's port");
                self.link = Link::Disconnected { retry_at: now + crate::link::RECONNECT };
            }
            Event::PortClosed => self.link_lost("The controller's port closed.", now, &mut effects),
            Event::Ports(ports) => self.ports_found(ports, &mut effects),
            Event::Health { cpu } => self.health(cpu, &mut effects),
        }
        self.pump(now, &mut effects);
        self.status_if_changed(&mut effects);
        effects
    }

    pub(crate) fn connection_status(&self) -> ConnectionStatus {
        match &self.link {
            Link::Connected { port } => ConnectionStatus {
                status: LinkStatus::Connected,
                port: port.clone(),
                fake_servo_status: self.fake_servo,
            },
            _ => ConnectionStatus { fake_servo_status: self.fake_servo, ..ConnectionStatus::disconnected() },
        }
    }

    pub(crate) fn units(&self) -> Units {
        Units::from_setting(&self.settings.text_or_empty("Computed Settings", "units"))
    }

    pub(crate) fn set_setting(&mut self, section: &str, key: &str, value: Value) {
        Arc::make_mut(&mut self.settings).set_value(section, key, value);
    }

    pub fn snapshot(&self) -> Snapshot {
        let running = self.run == RunState::Running;
        let (link, port) = match &self.link {
            Link::Connected { port } => (LinkStatus::Connected, port.clone()),
            _ => (LinkStatus::Disconnected, "none".to_string()),
        };
        Snapshot {
            link,
            port,
            upload_flag: i32::from(running),
            job_in_progress: running,
            manual_z_axis_adjust: false,
            pause_button: PauseButton::Pause,
            controller_firmware_version: self.firmware_version,
            controller_board: self.board.clone(),
            firmware_build: wc_firmware::boot::firmware_build(self.firmware_version, self.rewrite_notice),
            host_address: self.host_address.clone(),
            positioning: self.positioning,
            current_tool: self.tool,
            gcode_index: self.index,
            gcode_line: self.gcode.get(self.index).cloned().unwrap_or_default(),
            gcode_count: self.gcode.len(),
            ports: self.ports.clone(),
            settings: Shared(self.settings.clone()),
        }
    }
}
