//! The serial link (B1-B8): opening the port, the boot handshake, flow control, Stop's resync and the line
//! classifier's consequences. Python split this between `serialPortThread.py` and `messageProcessor.py`.

use std::time::Duration;

use wc_firmware::boot::{
    BOOT_BANNER_WINDOW_MS, EEPROM_READ_FAIL, EEPROM_REWRITE_NOTICE, board_banner, ends_boot, is_boot_banner,
};
use wc_firmware::settings::{Sync, boot_push, sync_reported};
use wc_firmware::upgrade::migration_push;
use wc_firmware::{Reply, classify, split_glued};
use wc_gcode::firmware_gcode::{only_selects_tool, tool_change_line};
use wc_gcode::sanitise::{problem_with, sanitise};
use wc_gcode::shift::positioning_set_by;
use wc_protocol::main::{ActivateModal, Alarm, ClearAlarm, ControllerMessage, Units};

use crate::job::Flow;
use crate::machine::{Controller, Effect, Link, Machine, Queued, alert, broadcast};

/// The 5 s poller Python used to reopen the port.
pub const RECONNECT: Duration = Duration::from_secs(5);
/// Python sleeps at least this long between writes so the Arduino keeps up.
pub const LINE_GAP: Duration = Duration::from_millis(50);
pub const RESYNC_DELAY: Duration = Duration::from_millis(250);
/// Longer than a reset takes to print its banner, shorter than the link timeout.
pub const BOOT_WAIT: Duration = Duration::from_secs(3);
/// No line at all for this long means the link is gone; the firmware reports status every 200 ms.
pub const LINK_TIMEOUT: Duration = Duration::from_secs(5);

const EEPROM_RESTORED: &str = "The controller lost its settings (EEPROM read fail), so WebControl restored them. \
                               WebControl doesn't keep Z-axis limits, so set them again if you use them.";
const TOUCH_Z_FAILED: &str =
    "The probe did not touch the plate, so Z was not zeroed. Check the plate and its clip, then try again.";
const CONNECTION_LOST: &str = "Message: USB connection lost. This has likely caused the machine to loose it's \
                               calibration, which can cause erratic behavior. It is recommended to stop the program, \
                               remove the sled, and perform the chain calibration process. Press Continue to \
                               override and proceed with the cut.";
const RESTARTED: &str = "The controller restarted, so the job was stopped. Check the sled before starting again.";

fn controller_message(text: &str) -> Effect {
    broadcast(&ControllerMessage(text.into()))
}

impl Machine {
    pub(crate) fn port_opened(&mut self, port: String, now: Duration, effects: &mut Vec<Effect>) {
        self.controller = Controller::new(now);
        self.controller.boot_deadline = Some(now + BOOT_WAIT);
        self.controller.buffering = self.settings.number("Maslow Settings", "bufferOn") != 0.0;
        self.job.controller_reset();
        effects.push(controller_message(&format!("Connected on port {port}")));
        self.link = Link::Connected { port };
        effects.push(broadcast(&self.connection_status()));
    }

    /// The port is gone: a running job stops, since the lines in the controller's buffer were lost.
    pub(crate) fn link_lost(&mut self, why: &str, now: Duration, effects: &mut Vec<Effect>) {
        if !matches!(self.link, Link::Connected { .. }) {
            return;
        }
        tracing::warn!(why, "controller link lost");
        if self.job.flow.upload_flag() > 0 {
            effects.push(broadcast(&ActivateModal::titled_alert("Connection Lost", CONNECTION_LOST)));
            self.pause_for_lost_link(effects);
        } else {
            effects.push(broadcast(&Alarm { message: "Alarm: Connection Failed or Invalid Firmware".into() }));
        }
        self.probing = false;
        self.link = Link::Disconnected { retry_at: now + RECONNECT };
        effects.push(Effect::ClosePort);
        effects.push(broadcast(&self.connection_status()));
    }

    pub(crate) fn tick(&mut self, now: Duration, effects: &mut Vec<Effect>) {
        self.upgrade_tick(now, effects);
        if let Link::Disconnected { retry_at } = self.link
            && now >= retry_at
        {
            let port =
                self.fixed_port.clone().unwrap_or_else(|| self.settings.text_or_empty("Maslow Settings", "COMport"));
            self.link = match port.is_empty() {
                true => Link::Disconnected { retry_at: now + RECONNECT },
                false => {
                    effects.push(Effect::OpenPort(port));
                    Link::Opening
                }
            };
        }
        if !matches!(self.link, Link::Connected { .. }) {
            return;
        }
        if now.saturating_sub(self.controller.last_line) > LINK_TIMEOUT {
            return self.link_lost("no controller line for 5 s", now, effects);
        }
        let controller = &mut self.controller;
        if controller.boot_deadline.is_some_and(|deadline| now >= deadline) {
            tracing::info!("no boot message from the controller; connecting anyway");
            self.handshake(now);
            self.controller.ready = true;
        }
        let controller = &mut self.controller;
        if controller.boot_in_progress && controller.last_banner.is_some_and(|at| now.saturating_sub(at) > BOOT_WAIT) {
            controller.boot_in_progress = false;
            controller.ready = true;
        }
        if controller.resync_after.is_some_and(|after| now >= after) {
            controller.caught_up = true;
        }
    }

    /// B2: on each boot ask for the version, then units, G90, every setting, and `$$` to verify.
    fn handshake(&mut self, now: Duration) {
        self.upgrade_restarted(now);
        let units = if self.units() == Units::Inches { "G20 " } else { "G21 " };
        let line = |text: &str| Queued::Line { text: text.into(), boot: true };
        let controller = &mut self.controller;
        controller.boot_deadline = None;
        controller.queue.clear();
        controller.queue.extend([line("B05 "), line(units), line("G90 "), Queued::SettingsPush, line("$$")]);
    }

    fn boot(&mut self, now: Duration, effects: &mut Vec<Effect>) {
        tracing::info!("controller boot");
        if matches!(self.job.flow, Flow::Streaming | Flow::Draining) {
            effects.push(alert(RESTARTED));
            self.end_job();
        }
        let (eeprom_reset, buffering) = (self.controller.eeprom_reset, self.controller.buffering);
        self.controller = Controller { eeprom_reset, buffering, ..Controller::new(now) };
        self.rewrite_notice = false;
        self.job.controller_reset();
        self.probing = false;
        self.handshake(now);
    }

    fn watch_for_boot(&mut self, line: &str, now: Duration, effects: &mut Vec<Effect>) {
        if line.contains(EEPROM_REWRITE_NOTICE) {
            self.rewrite_notice = true;
        }
        if !is_boot_banner(line) {
            return;
        }
        self.board = board_banner(line).or(self.board.take());
        let window = Duration::from_millis(BOOT_BANNER_WINDOW_MS);
        let last = self.controller.last_banner;
        if !self.controller.boot_in_progress || last.is_none_or(|at| now.saturating_sub(at) > window) {
            self.boot(now, effects);
        }
        self.controller.boot_in_progress = !ends_boot(line);
        self.controller.last_banner = Some(now);
        self.upgrade_banner(line, now, effects);
    }

    pub(crate) fn controller_line(&mut self, text: &str, now: Duration, effects: &mut Vec<Effect>) {
        if !matches!(self.link, Link::Connected { .. }) {
            return;
        }
        self.controller.last_line = now;
        effects.push(Effect::Log(text.to_string()));
        split_glued(text).iter().for_each(|line| self.one_line(line, now, effects));
    }

    fn one_line(&mut self, line: &str, now: Duration, effects: &mut Vec<Effect>) {
        self.watch_for_boot(line, now, effects);
        if self.controller.boot_deadline.is_some() && line == "ok" {
            // A board that did not reset on open, answering all the same.
            self.handshake(now);
        }
        if line.contains(EEPROM_READ_FAIL) {
            self.controller.eeprom_reset = true;
            if !self.controller.queue.contains(&Queued::SettingsPush) {
                let read_back = Queued::Line { text: "$$".into(), boot: true };
                self.controller.queue.extend([Queued::SettingsPush, read_back]);
            }
        }
        let reply = classify(line);
        self.acknowledge(line, &reply, effects);
        self.react(line, reply, now, effects);
    }

    fn acknowledge(&mut self, line: &str, reply: &Reply, effects: &mut Vec<Effect>) {
        let controller = &mut self.controller;
        if controller.resync_after.is_some() {
            if line.starts_with('<') && controller.caught_up {
                controller.resync_after = None;
                controller.accounting.clear();
                controller.ready = true;
            }
            return;
        }
        if controller.boot_in_progress || !matches!(reply, Reply::Ok | Reply::Error(_)) {
            return;
        }
        controller.ready = true;
        let acked = controller.accounting.acked();
        // Identity, not position: a `~` sent mid-probe is acked first with an entry of its own.
        if acked.is_some_and(|sent| Some(sent.id) == controller.probe) {
            controller.probe = None;
            self.probing = false;
            if !matches!(reply, Reply::Ok) {
                effects.push(broadcast(&ActivateModal::titled_alert("Touch Z Failed", TOUCH_Z_FAILED)));
            }
        }
    }

    /// What a line means beyond its ack: the messageProcessor's state changes and the UIProcessor's pushes.
    fn react(&mut self, line: &str, reply: Reply, now: Duration, effects: &mut Vec<Effect>) {
        match reply {
            Reply::Ok => {}
            Reply::Status(status) => self.status_report(status, now, effects),
            Reply::Misread(text) => tracing::debug!(text, "misread controller report"),
            Reply::PositionError(error) => self.position_error(error, effects),
            Reply::Setting { key, value } => self.reported_setting(line, key, value),
            Reply::Measured(mm) if self.upgrade.collects_chains() => self.upgrade_measured(mm),
            Reply::Measured(mm) => self.measured(mm, now, effects),
            Reply::FakeServo(on) => self.fake_servo = on,
            Reply::MaslowPaused => self.firmware_paused(effects),
            Reply::ToolChange { tool } => self.tool_change_prompt(line, tool, effects),
            Reply::Message(text) => self.controller_notice(&text, effects),
            Reply::Alarm(text) => {
                self.halt(now, effects);
                let message = match text {
                    _ if text.contains("The sled is not keeping up") => "Alarm: Sled Not Keeping Up".into(),
                    _ if text.contains("Position Lost") => "Alarm: Position Lost.  Reset Chains.".into(),
                    other => other,
                };
                effects.push(broadcast(&Alarm { message }));
            }
            Reply::BufferOverflow => {
                self.halt(now, effects);
                effects.push(broadcast(&Alarm { message: "Alarm: Controller Buffer Overflow".into() }));
            }
            Reply::FirmwareVersion(version) => {
                effects.push(broadcast(&ClearAlarm::new()));
                effects.push(controller_message(line));
                self.firmware_version = version.unwrap_or(self.firmware_version);
                if let Some(version) = version {
                    self.upgrade_version(version, now, effects);
                }
            }
            Reply::Error(_) | Reply::Other(_) => {
                self.upgrade_report(line);
                if !self.pid_line(line, now, effects) && !line.is_empty() {
                    effects.push(controller_message(line));
                }
            }
        }
    }

    /// `Message:` lines hold the job: the firmware likely pauses next.
    fn controller_notice(&mut self, text: &str, effects: &mut Vec<Effect>) {
        self.hold_streaming();
        let body = text.get(9..).unwrap_or_default();
        if text.contains("adjust Z-Axis") {
            self.manual_z_prompt();
            effects.push(broadcast(&ActivateModal::notification(body, true)));
        } else if text.contains("Unable to find valid") {
            // Setting the sprockets to zero reports a zero chain length, which is expected.
            if !(text.contains("lengths 0.00") || text.contains(", 0.00")) {
                effects.push(broadcast(&Alarm { message: text.into() }));
            }
        } else {
            effects.push(broadcast(&ActivateModal::notification(body, false)));
        }
    }

    fn reported_setting(&mut self, line: &str, key: u32, value: f64) {
        let sync = sync_reported(&self.settings, self.firmware_version, key, value);
        self.upgrade_setting(line, if let Sync::Rewrite(text) = &sync { Some(text) } else { None });
        match sync {
            Sync::Adopt { section, key, value } => self.set_setting(&section, &key, value),
            Sync::Rewrite(line) => self.controller.queue.push_back(Queued::Line { text: line, boot: false }),
            Sync::Agrees => {}
        }
    }

    /// The firmware stops itself on these and never acks the lines it held (B6).
    fn halt(&mut self, now: Duration, effects: &mut Vec<Effect>) {
        self.rewind_job(effects);
        if self.job.flow.in_job() {
            self.job.flow = Flow::Held;
        }
        self.await_resync(now);
    }

    /// The controller flushed its buffer, but acks sent before it did may still be in flight.
    pub(crate) fn await_resync(&mut self, now: Duration) {
        let controller = &mut self.controller;
        controller.accounting.clear();
        controller.probe = None;
        controller.ready = false;
        controller.resync_after = Some(now + RESYNC_DELAY);
        controller.caught_up = false;
        self.probing = false;
    }

    pub(crate) fn realtime(&mut self, text: &str, now: Duration, effects: &mut Vec<Effect>) {
        if !matches!(self.link, Link::Connected { .. }) {
            return;
        }
        let message = format!("{text}\n");
        effects.push(Effect::Realtime(message.clone().into_bytes()));
        effects.push(Effect::Log(format!("Sent: {text}")));
        self.controller.last_write = Some(now);
        if text == "!" {
            self.await_resync(now);
        } else {
            self.controller.accounting.realtime_sent(message.len());
            self.controller.ready = false;
        }
    }

    pub(crate) fn write(&mut self, text: &str, index: Option<usize>, now: Duration, effects: &mut Vec<Effect>) -> u64 {
        let message = format!("{text}\n");
        let sent = self.controller.accounting.sent(message.len(), index);
        self.controller.ready = false;
        self.controller.last_write = Some(now);
        if let Some(mode) = positioning_set_by(&message) {
            self.positioning = mode;
        }
        let setting = text.strip_prefix('$').and_then(|setting| setting.split_once('='));
        if let Some((key, value)) = setting.and_then(|(key, value)| Some((key.parse().ok()?, value.trim()))) {
            self.kinematics.update_setting(key, value);
        }
        effects.push(Effect::Write(message.into_bytes()));
        effects.push(Effect::Log(format!("Sent: {text}")));
        sent.id
    }

    /// Connected, not resyncing, not held by a `Delay`, and 50 ms since the last write.
    fn may_write(&self, now: Duration) -> bool {
        let controller = &self.controller;
        matches!(self.link, Link::Connected { .. })
            && controller.resync_after.is_none()
            && controller.hold_until.is_none_or(|until| now >= until)
            && controller.last_write.is_none_or(|at| now >= at + LINE_GAP)
    }

    /// The controller has acked everything: a queued command, or an unbuffered job line, may go.
    fn can_write(&self, now: Duration) -> bool {
        self.may_write(now) && self.controller.accounting.is_empty() && self.controller.ready
    }

    /// Buffered, a job line goes whenever it fits the controller's free bytes.
    fn can_stream(&self, now: Duration) -> bool {
        let room = match self.controller.buffering {
            true => self.next_line_fits(),
            false => self.controller.accounting.is_empty() && self.controller.ready,
        };
        self.job.flow == Flow::Streaming
            && !self.gcode.is_empty()
            && self.controller.queue.is_empty()
            && self.may_write(now)
            && room
    }

    /// Writes whatever the controller has room for: a queued command first, else the job's next line.
    pub(crate) fn pump(&mut self, now: Duration, effects: &mut Vec<Effect>) {
        while self.can_write(now) {
            let Some(next) = self.controller.queue.pop_front() else {
                break;
            };
            let written = match next {
                Queued::SettingsPush => {
                    self.expand_settings_push(effects);
                    false
                }
                Queued::Line { text, .. } => self.send_queued(&text, now, effects),
                Queued::Delay(wait) => {
                    self.controller.hold_until = Some(now + wait);
                    false
                }
                Queued::Mark(mark) => {
                    self.mark(mark, effects);
                    false
                }
            };
            if written {
                return;
            }
        }
        if self.can_stream(now) {
            self.send_next_line(now, effects);
        }
    }

    fn expand_settings_push(&mut self, effects: &mut Vec<Effect>) {
        let eeprom_reset = std::mem::take(&mut self.controller.eeprom_reset);
        let migration = self.upgrade.migrating();
        let build = wc_firmware::boot::firmware_build(self.firmware_version, self.rewrite_notice);
        let push = match migration {
            Some(migration) => migration_push(&self.settings, self.firmware_version, build.as_deref(), migration),
            None => boot_push(&self.settings, self.firmware_version, eeprom_reset),
        };
        push.adopted.into_iter().for_each(|(section, key, value)| self.set_setting(&section, &key, value));
        push.lines
            .into_iter()
            .rev()
            .for_each(|text| self.controller.queue.push_front(Queued::Line { text, boot: true }));
        if eeprom_reset && migration.is_none() {
            effects.push(alert(EEPROM_RESTORED));
        }
    }

    /// `_sendQueuedCommand`: false when the command was dropped instead of written.
    fn send_queued(&mut self, command: &str, now: Duration, effects: &mut Vec<Effect>) -> bool {
        let command = sanitise(command);
        if command.is_empty() {
            return false;
        }
        let command = format!("{command} ");
        let to_send = tool_change_line(&command, self.job.next_tool, self.job.tool).line;
        if let Some(problem) = problem_with(&to_send) {
            let reason = format!("That command {}", problem.reason());
            effects.push(broadcast(&ActivateModal::titled_alert(&format!("Command {}", problem.title()), &reason)));
            self.probing = self.probing && !command.contains("G38.2");
            return false;
        }
        let command = self.manage_tool_change(&command);
        if only_selects_tool(&command) {
            return false;
        }
        let id = self.write(&command, None, now, effects);
        if command.starts_with("$RST") {
            // The firmware never acks a reset of its EEPROM.
            self.controller.accounting.acked();
            self.controller.ready = true;
        }
        if command.contains("G38.2") {
            self.controller.probe = Some(id);
            self.probing = true;
        }
        self.follow_units_in(&command, effects);
        true
    }

    pub(crate) fn queue(&mut self, lines: impl IntoIterator<Item = String>) {
        self.controller.queue.extend(lines.into_iter().map(|text| Queued::Line { text, boot: false }));
    }

    /// Stop drops what the user queued and any wait; the boot's handshake and settings push still go out.
    pub(crate) fn drop_user_commands(&mut self) {
        self.controller.queue.retain(|queued| {
            matches!(queued, Queued::Line { boot: true, .. } | Queued::SettingsPush | Queued::Mark(_))
        });
        self.controller.hold_until = None;
    }

    pub(crate) fn ports_found(&mut self, ports: Vec<String>, effects: &mut Vec<Effect>) {
        self.ports = if ports.is_empty() { vec!["None".into()] } else { ports };
        effects.push(broadcast(&wc_protocol::settings::UpdatePorts(self.ports.clone())));
    }
}
