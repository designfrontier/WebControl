use std::collections::VecDeque;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Map, Value, json};
use wc_config::Settings;
use wc_firmware::Accounting;
use wc_firmware::image::Image;
use wc_gcode::Positioning;
use wc_gcode::board::Board;
use wc_gcode::restart::positioning_word;
use wc_protocol::main::{ActivateModal, ConnectionStatus, HostAddress, LinkStatus, PauseButton, StatusMessage, Units};

use crate::job::Flow;
use wc_protocol::system::FirmwareUpgrade;
use wc_protocol::{Action, Envelope, Message, Push};

use crate::edits::{Request as Edit, Response};
use crate::files::{Preview, Reply, Request, bed_board};
use crate::gpio::{Button, Leds};
use crate::job::Job;
use crate::pid::Pid;
use crate::setup::Measure;
use crate::ui::{ErrorTracker, PositionTracker};
use crate::upgrade::{FirmwareJob, JobResult, Mark, Upgrade};
use crate::work::{Done, Lanes, Work};

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
    /// The front page's unit switches.
    UpdateSetting {
        setting: String,
        value: Value,
    },
    /// A settings write that needs an answer; the shell pairs the `Respond` effect with the edit by `id`.
    Edit {
        id: u64,
        request: Edit,
    },
    ClientConnected {
        sid: String,
    },
    /// A client fell behind its pushes and missed some; it hears the current state again.
    ClientResync {
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
    /// A [`FirmwareJob`] the shell ran.
    FirmwareDone(JobResult),
    CheckForGcodeUpdate,
    CheckForBoardUpdate,
    /// A gcode library directory now exists; the UI lists it.
    DirectoryCreated(String),
    /// An HTTP route's request; the shell gives each its `id` and routes the `Effect::Reply` back.
    Request {
        id: u64,
        request: Request,
    },
    /// A GPIO button, debounced.
    Button(Button),
    /// A [`Work`] the shell ran.
    Computed(Done),
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
    Respond {
        id: u64,
        response: Response,
    },
    /// Write `webcontrol.json`; only the latest one pending matters.
    SaveSettings(Arc<Settings>),
    /// Run off the machine task; the port is closed first.
    Firmware(FirmwareJob),
    /// `createDirectory`: make this gcode library directory, then report `Event::DirectoryCreated`.
    CreateDirectory(String),
    Reply {
        id: u64,
        reply: Reply,
    },
    /// The GPIO status LEDs, whenever they change.
    Leds(Leds),
    /// Run off the machine task, then report `Event::Computed`.
    Compute(Work),
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

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Link {
    Disconnected {
        retry_at: Duration,
    },
    Opening,
    Connected {
        port: String,
    },
    /// avrdude has the port; nothing reopens it until the flash is over.
    HandedOff,
}

/// A command waiting for the controller to go idle; the boot's own lines survive a Stop.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Queued {
    Line {
        text: String,
        boot: bool,
    },
    /// Expanded once the `$$` before it has answered, so the writes know the firmware and what it holds.
    SettingsSync,
    /// Nothing more is written for this long, as Python's `time.sleep` in the action held the queue.
    Delay(Duration),
    /// The upgrade acts here once every line before it is answered.
    Mark(Mark),
    /// Expanded once every line before it is answered, so the bit's height is known.
    Raise(crate::clearance::Raise),
}

/// What readers see; every field is derived from the machine's state, never stored separately.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub link: LinkStatus,
    pub port: String,
    pub fake_servo: bool,
    pub upload_flag: i32,
    pub job_in_progress: bool,
    pub manual_z_axis_adjust: bool,
    /// A G38.2 is queued or descending; only Stop and actions that move nothing run.
    pub probing: bool,
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
    pub gcode: Shared<Vec<String>>,
    pub gcode_file: Option<PathBuf>,
    /// What `/sendGCode` last sent.
    pub custom_gcode: Option<String>,
    /// The file routes refuse to replace the gcode while this holds (their 409).
    pub gcode_locked: bool,
    pub preview: Shared<Preview>,
    pub board: Shared<Board>,
    /// The sled's X and Y from the last status report.
    pub sled: (f64, f64),
    pub ports: Vec<String>,
    pub settings: Shared<Settings>,
    pub firmware_upgrade: FirmwareUpgrade,
}

impl Snapshot {
    /// What `connectionStatus` would push now.
    pub fn connection_status(&self) -> ConnectionStatus {
        ConnectionStatus { status: self.link, port: self.port.clone(), fake_servo_status: self.fake_servo }
    }

    pub fn status(&self) -> StatusMessage {
        StatusMessage {
            upload_flag: self.upload_flag,
            positioning_mode: u8::from(self.positioning == Positioning::Relative),
            current_tool: self.current_tool,
            job_in_progress: self.job_in_progress,
            manual_z_axis_adjust: self.manual_z_axis_adjust,
            probing: self.probing,
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
            "probing": self.probing,
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

/// A G38.2 the controller has not answered yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Probe {
    pub id: u64,
    /// The job's line, for a probe streamed from the file.
    pub index: Option<usize>,
    /// The firmware printed `z axis zeroed`, which only a touch does.
    pub zeroed: bool,
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
    pub probe: Option<Probe>,
    pub eeprom_reset: bool,
    /// The boot's `$$` reply, read until its `SettingsSync`.
    pub reported: Vec<(u32, f64)>,
    /// A re-initialising `$` line went out on this boot, so the controller must restart before it moves.
    pub axes_reinitialised: bool,
    /// The restart did not make `$38` stick, so no `$$` on this boot writes it again.
    pub reinit_suppressed: bool,
    /// A `Delay` holds writes until then.
    pub hold_until: Option<Duration>,
    /// `bufferOn` as the port opened: job lines go out while they fit, not one per `ok`.
    pub buffering: bool,
    /// A raise waiting for a status report to tell the bit's height is refused then.
    pub raise_deadline: Option<Duration>,
    /// Stop's `!` makes the firmware reload its position; that reload's unplaceable-sled message does not hold.
    pub stop_reload: bool,
    /// Whether the line it waits for ever reached the controller.
    pub watch: crate::lost::Watch,
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
            reported: Vec::new(),
            axes_reinitialised: false,
            reinit_suppressed: false,
            hold_until: None,
            buffering: false,
            raise_deadline: None,
            stop_reload: false,
            watch: crate::lost::Watch::default(),
        }
    }
}

#[derive(Clone)]
pub struct Machine {
    pub(crate) host_address: String,
    /// Opened in place of the `COMport` setting: the simulator's port.
    pub(crate) fixed_port: Option<String>,
    pub(crate) settings: Arc<Settings>,
    /// The settings as last handed to the writer.
    pub(crate) saved: Arc<Settings>,
    pub(crate) link: Link,
    pub(crate) controller: Controller,
    pub(crate) firmware_version: f64,
    pub(crate) rewrite_notice: bool,
    pub(crate) board: Option<String>,
    pub(crate) fake_servo: bool,
    pub(crate) positioning: Positioning,
    /// A G38.2 (Touch Z, typed or from the file) is queued or descending.
    pub(crate) probing: bool,
    pub(crate) job: Job,
    /// The alarm the controller raised, shown until Stop or Play clears it; nothing that moves is written meanwhile.
    pub(crate) alarm: Option<crate::alarm::Hold>,
    /// Gcode loads being built off the machine task; Play waits for them.
    pub(crate) loads: usize,
    pub(crate) leds: Leds,
    /// The LEDs as last handed to the shell.
    pub(crate) leds_shown: Leds,
    pub(crate) gcode: Arc<Vec<String>>,
    pub(crate) index: usize,
    pub(crate) gcode_file: Option<PathBuf>,
    pub(crate) z_moves: Vec<usize>,
    pub(crate) preview: Arc<Preview>,
    /// The loaded file's truncate setting, which the home shift keeps to.
    pub(crate) truncate: Option<u32>,
    pub(crate) custom_gcode: Option<String>,
    pub(crate) workpiece: Arc<Board>,
    pub(crate) settings_dirty: bool,
    pub(crate) ports: Vec<String>,
    pub(crate) position: PositionTracker,
    pub(crate) bit: crate::clearance::BitHeight,
    pub(crate) errors: ErrorTracker,
    pub(crate) reported_buffer: i64,
    pub(crate) last_status: Option<StatusMessage>,
    /// What the next B10 `[Measure` reply is for.
    pub(crate) measure: Option<Measure>,
    pub(crate) pid: Pid,
    pub(crate) calibration: crate::calibration::Results,
    /// The forward-kinematics model for the computed sled position, kept current by every `$` write.
    pub(crate) kinematics: wc_kinematics::Kinematics,
    pub(crate) upgrade: Upgrade,
    pub(crate) restart: crate::sync::Restart,
    pub(crate) lanes: Lanes,
}

pub(crate) fn broadcast<M: Message>(message: &M) -> Effect {
    Effect::Push(Push::message(Envelope::json(message)))
}

pub(crate) fn alert(text: &str) -> Effect {
    broadcast(&ActivateModal::alert(text))
}

impl Machine {
    pub fn new(host_address: &str, settings: Settings) -> Machine {
        let units = Units::from_setting(&settings.text_or_empty("Computed Settings", "units"));
        let workpiece = Arc::new(bed_board(&settings));
        let kinematics = crate::ui::computed_model(&settings);
        let settings = Arc::new(settings);
        Machine {
            host_address: host_address.into(),
            fixed_port: None,
            workpiece,
            preview: Arc::new(Preview::none(units)),
            saved: settings.clone(),
            settings,
            link: Link::Disconnected { retry_at: Duration::ZERO },
            controller: Controller::new(Duration::ZERO),
            firmware_version: 0.0,
            rewrite_notice: false,
            board: None,
            fake_servo: false,
            positioning: Positioning::Absolute,
            probing: false,
            job: Job::default(),
            alarm: None,
            loads: 0,
            leds: Leds::default(),
            leds_shown: Leds::default(),
            gcode: Arc::new(Vec::new()),
            index: 0,
            gcode_file: None,
            z_moves: vec![0],
            truncate: None,
            custom_gcode: None,
            settings_dirty: false,
            ports: Vec::new(),
            position: PositionTracker::default(),
            bit: crate::clearance::BitHeight::new(),
            errors: ErrorTracker::default(),
            reported_buffer: 127,
            last_status: None,
            measure: None,
            pid: Pid::default(),
            calibration: crate::calibration::Results::default(),
            kinematics,
            upgrade: Upgrade::new(Image::embedded()),
            restart: crate::sync::Restart::None,
            lanes: Lanes::default(),
        }
    }

    /// Upgrades with `image` in place of the one embedded at build time.
    pub fn with_firmware(mut self, image: Option<Image>) -> Machine {
        self.upgrade.image = image;
        self
    }

    /// Always opens `port`, whatever `COMport` says.
    pub fn on_port(self, port: &str) -> Machine {
        Machine { fixed_port: Some(port.into()), ..self }
    }

    /// Loads lines as `/openGCode` would, without the preview; transcripts and tests use it directly.
    pub fn load_gcode(&mut self, text: &str) {
        self.gcode = Arc::new(wc_gcode::load::filter(text, None).unwrap_or_default());
        self.index = 0;
    }

    pub fn handle(&mut self, event: Event, now: Duration) -> Vec<Effect> {
        let mut effects = Vec::new();
        match event {
            Event::ClientConnected { .. } => {
                effects.push(broadcast(&HostAddress { host_address: self.host_address.clone() }));
                effects.push(broadcast(&self.connection_status()));
            }
            Event::ClientResync { sid } => self.resync(&sid, &mut effects),
            Event::Action(action) | Event::Stop(action) => self.action(&action, now, &mut effects),
            Event::SettingRequest { section, setting } => self.setting_request(&section, &setting, &mut effects),
            Event::UpdateSetting { setting, value } => self.update_setting(&setting, &value, &mut effects),
            Event::Edit { id, request } => {
                let response = self.edit(request, now, &mut effects);
                effects.push(Effect::Respond { id, response });
            }
            Event::ControllerLine(line) => self.controller_line(&line, now, &mut effects),
            Event::Tick => self.tick(now, &mut effects),
            Event::PortOpened { .. } | Event::PortFailed { .. } if self.link == Link::HandedOff => {
                // An open still on its way when a rollback took the link; avrdude needs the port.
                effects.extend(matches!(event, Event::PortOpened { .. }).then_some(Effect::ClosePort));
            }
            Event::PortOpened { port } => self.port_opened(port, now, &mut effects),
            Event::PortFailed { port, error } => {
                tracing::debug!(port, error, "could not open the controller's port");
                self.link = Link::Disconnected { retry_at: now + crate::link::RECONNECT };
            }
            Event::PortClosed => self.link_lost("The controller's port closed.", now, &mut effects),
            Event::Ports(ports) => self.ports_found(ports, &mut effects),
            Event::Health { cpu } => self.health(cpu, &mut effects),
            Event::FirmwareDone(result) => self.firmware_done(result, now, &mut effects),
            Event::CheckForGcodeUpdate => self.gcode_update_requested(&mut effects),
            Event::CheckForBoardUpdate => self.push_board(&mut effects),
            Event::DirectoryCreated(name) => self.directory_created(name, &mut effects),
            Event::Request { id, request } => {
                let reply = self.request(request, &mut effects);
                effects.push(Effect::Reply { id, reply });
            }
            Event::Button(button) => {
                let resumes = matches!(self.job.flow, Flow::Paused | Flow::Draining) || self.job.manual_z.is_some();
                let action = Action::new(button.command(resumes), Value::from(""), Value::from(""));
                self.action(&action, now, &mut effects);
            }
            Event::Computed(done) => self.computed(done, &mut effects),
        }
        self.pump(now, &mut effects);
        self.status_if_changed(&mut effects);
        let changed = !Arc::ptr_eq(&self.settings, &self.saved) && *self.settings != *self.saved;
        if std::mem::take(&mut self.settings_dirty) || changed {
            effects.push(Effect::SaveSettings(self.settings.clone()));
        }
        self.saved = self.settings.clone();
        if self.leds != self.leds_shown {
            self.leds_shown = self.leds;
            effects.push(Effect::Leds(self.leds));
        }
        effects
    }

    /// `handle`, with each [`Effect::Compute`] run at once and its effects in its place, for tests and tools.
    pub fn handle_inline(&mut self, event: Event, now: Duration) -> Vec<Effect> {
        let effects = self.handle(event, now);
        effects
            .into_iter()
            .flat_map(|effect| match effect {
                Effect::Compute(work) => self.handle_inline(work.run(), now),
                other => vec![other],
            })
            .collect()
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
        self.settings_dirty |= Arc::make_mut(&mut self.settings).set_value(section, key, value);
    }

    pub fn snapshot(&self) -> Snapshot {
        let (link, port) = match &self.link {
            Link::Connected { port } => (LinkStatus::Connected, port.clone()),
            _ => (LinkStatus::Disconnected, "none".to_string()),
        };
        Snapshot {
            link,
            port,
            fake_servo: self.fake_servo,
            upload_flag: self.job.flow.upload_flag(),
            job_in_progress: self.job.flow.in_job(),
            manual_z_axis_adjust: self.job.manual_z.is_some(),
            probing: self.probing,
            pause_button: self.job.pause_button(),
            controller_firmware_version: self.firmware_version,
            controller_board: self.board.clone(),
            firmware_build: wc_firmware::boot::firmware_build(self.firmware_version, self.rewrite_notice),
            host_address: self.host_address.clone(),
            positioning: self.positioning,
            current_tool: u32::try_from(self.job.tool).unwrap_or(0),
            gcode_index: self.index,
            gcode_line: self.gcode.get(self.index).cloned().unwrap_or_default(),
            gcode_count: self.gcode.len(),
            gcode: Shared(self.gcode.clone()),
            gcode_file: self.gcode_file.clone(),
            custom_gcode: self.custom_gcode.clone(),
            gcode_locked: self.gcode_locked(),
            preview: Shared(self.preview.clone()),
            board: Shared(self.workpiece.clone()),
            sled: (self.position.x, self.position.y),
            ports: self.ports.clone(),
            settings: Shared(self.settings.clone()),
            firmware_upgrade: self.upgrade.view(),
        }
    }
}
