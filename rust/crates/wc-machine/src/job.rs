//! The job: streaming the file and every way it pauses (Pause, M0/M1, a tool change, manual Z, an alarm, a refused
//! line, a lost link), then Play, Resume and Stop, as `actions.py`, `serialPortThread.py` and
//! `messageProcessor.py` keep it.

use std::time::Duration;

use serde_json::Value;
use wc_config::python_float;
use wc_firmware::settings::reinitialises_axes;
use wc_gcode::firmware_gcode::{only_selects_tool, pauses_firmware, tool_change_line};
use wc_gcode::position::{Position, line_target, word};
use wc_gcode::restart::{GcodeState, Options, Restart, continue_arc, sequence, state_at};
use wc_gcode::sanitise::{LineProblem, problem_with, sanitise};
use wc_gcode::shift::{HomeShift, positioning_set_by, shift_to_home};
use wc_gcode::{Positioning, py};
use wc_protocol::main::{ActivateModal, ClearAlarm, GcodePositionMessage, PauseButton, RequestedSetting, Units};

use crate::clearance::Raise;
use crate::gpio::Led;
use crate::machine::{Effect, Machine, Queued, alert, broadcast};

const SKIPPED_REINIT: &str = "sets chain-over-sprocket ($38), which re-initialises the motors mid-job, so it was \
                              skipped. Change it in Advanced Settings instead.";
const XY_UNKNOWN: &str = "Cannot return to this line: the file moves X or Y relatively (G91) before it sets \
                          them absolutely, so where the sled belongs depends on where the file started. Stop, then \
                          run the file from its first line.";

/// Where the job is; `uploadFlag` on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Flow {
    #[default]
    Idle,
    Streaming,
    /// The line the firmware pauses on has gone out, and nothing follows it.
    Draining,
    /// Pause, a line the controller cannot take, or a lost link.
    Paused,
    /// The controller paused or halted the job; Play resumes it.
    Held,
}

impl Flow {
    pub fn upload_flag(self) -> i32 {
        match self {
            Flow::Streaming => 1,
            Flow::Draining => -1,
            Flow::Paused => 2,
            Flow::Idle | Flow::Held => 0,
        }
    }

    pub fn in_job(self) -> bool {
        self != Flow::Idle
    }
}

/// What the job and the firmware's pauses leave behind; only `Machine::handle` writes it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Job {
    pub flow: Flow,
    /// `Maslow Paused`: the firmware runs nothing until `~`.
    pub firmware_paused: bool,
    /// Job lines may be buffered behind the firmware's pause, so a jog must not release it.
    pub holds_job_lines: bool,
    /// The firmware waits for Z by hand; `true` when that is a tool change.
    pub manual_z: Option<bool>,
    pub tool: i64,
    pub next_tool: i64,
    /// The tool before an M6 the firmware has not confirmed yet, which a Stop flushes.
    pub tool_before_change: Option<i64>,
    pub paused_units: Option<Units>,
    /// Where Pause left the job: the index and the last Z sent.
    pub paused_at: Option<(usize, f64)>,
    /// The last Z the file sent, as an absolute target.
    pub z_target: f64,
    /// The last streamed line's target, for `gcodePositionMessage`.
    pub streamed: Position,
    /// The file's modal G2 or G3 at a restart, until a file line settles the motion the restart left as G0 or G1.
    pub arc_after_restart: Option<u8>,
}

impl Default for Job {
    fn default() -> Job {
        Job {
            flow: Flow::Idle,
            firmware_paused: false,
            holds_job_lines: false,
            manual_z: None,
            tool: 0,
            next_tool: 0,
            tool_before_change: None,
            paused_units: None,
            paused_at: None,
            z_target: 0.0,
            streamed: Position { x: 0.0, y: 0.0, z: 0.0, feed_rate: None },
            arc_after_restart: None,
        }
    }
}

impl Job {
    /// A reset controller is not paused, waits for no Z by hand, and has tool 0.
    pub fn controller_reset(&mut self) {
        self.manual_z = self.manual_z.filter(|&tool_change| tool_change);
        self.firmware_paused = false;
        self.holds_job_lines = false;
        self.tool = 0;
        self.next_tool = 0;
        self.tool_before_change = None;
    }

    /// `pauseButtonLabel`: Resume whenever a job or a tool change is held.
    pub fn pause_button(&self) -> PauseButton {
        let held = self.flow.in_job() || self.manual_z.is_some();
        if self.flow == Flow::Streaming || !held { PauseButton::Pause } else { PauseButton::Resume }
    }
}

/// Why a restart at the index cannot be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unbuilt {
    /// No home, or a word the file cannot read.
    Unreadable,
    XyUnknown,
}

/// Nothing for the controller: empty, or whitespace only once G90.1 and G91.1 are left out.
pub(crate) fn blank(line: &str) -> bool {
    line.is_empty() || py::isspace(line)
}

pub(crate) fn pause_button(button: PauseButton) -> Effect {
    let value = serde_json::to_value(button).unwrap_or(Value::Null);
    broadcast(&RequestedSetting { setting: "pauseButtonSetting".into(), value })
}

impl Machine {
    /// `holdStreaming`: the controller said something that stops the job where it is.
    pub(crate) fn hold_streaming(&mut self) {
        let job = &mut self.job;
        job.holds_job_lines |= job.flow.upload_flag() > 0;
        if job.flow.in_job() {
            job.flow = Flow::Held;
        }
    }

    pub(crate) fn firmware_paused(&mut self, now: Duration, effects: &mut Vec<Effect>) {
        self.hold_streaming();
        self.job.firmware_paused = true;
        effects.push(pause_button(PauseButton::Resume));
        if self.job.manual_z == Some(true) {
            self.park_for_tool_change(now, effects);
        }
    }

    /// `Tool Change: Please insert tool N`: the firmware's word on the tool wins, as a Stop can flush an M6.
    pub(crate) fn tool_change_prompt(&mut self, line: &str, tool: Option<u32>, effects: &mut Vec<Effect>) {
        self.job.manual_z = Some(true);
        self.job.tool_before_change = None;
        self.hold_streaming();
        self.job.paused_units = Some(self.units());
        if let Some(tool) = tool {
            self.job.tool = i64::from(tool);
            self.job.next_tool = i64::from(tool);
        }
        self.park_tool_change_asked();
        let asked = tool.or(u32::try_from(self.job.next_tool).ok().filter(|&tool| tool > 0));
        effects.push(broadcast(&ActivateModal::tool_change(line.get(13..).unwrap_or_default(), asked)));
    }

    /// `$16=0`'s prompt to adjust Z by hand, which Resume confirms.
    pub(crate) fn manual_z_prompt(&mut self) {
        self.job.manual_z = Some(self.job.manual_z.unwrap_or(false));
        self.job.paused_units = Some(self.units());
    }

    /// The firmware stopped itself and dropped the lines it held; a job resumes from the oldest of them.
    pub(crate) fn rewind_job(&mut self, effects: &mut Vec<Effect>) {
        if !self.job.flow.in_job() {
            return;
        }
        if let Some(index) = self.controller.accounting.oldest_index() {
            self.index = index;
            self.push_gcode_position(effects);
        }
    }

    pub(crate) fn end_job(&mut self) {
        self.job.flow = Flow::Idle;
        self.job.arc_after_restart = None;
        self.index = 0;
    }

    pub(crate) fn release_firmware(&mut self, now: Duration, effects: &mut Vec<Effect>) {
        self.job.firmware_paused = false;
        self.job.holds_job_lines = false;
        self.realtime("~", now, effects);
    }

    /// Resume could build the restart that returns the job to its line.
    pub(crate) fn can_return_to_line(&self) -> bool {
        self.restart_state().is_ok()
    }

    /// The file's state before the index, which a restart returns to.
    fn restart_state(&self) -> Result<GcodeState, Unbuilt> {
        let number = |key: &str| self.settings.value("Advanced Settings", key).and_then(python_float);
        let home = number("homeX").zip(number("homeY"));
        let state = home.and_then(|(x, y)| state_at(&self.gcode, self.index, x, y, self.units()).ok());
        match state {
            Some(state) if state.xy_unknown => Err(Unbuilt::XyUnknown),
            Some(state) => Ok(state),
            None => Err(Unbuilt::Unreadable),
        }
    }

    /// Queues the restart, whose height is settled once the lines before it are answered; `None` if it cannot be built.
    fn queue_restart(&mut self, state: GcodeState, z_target: Option<f64>, change_tool: bool) -> Option<Restart> {
        let safe_height = self.safe_height_mm()?;
        let version = self.firmware_version;
        let options = Options {
            z_target,
            change_tool: change_tool && !self.skips_bit_changes(),
            current_tool: self.job.tool,
            safe_height_mm: safe_height,
            current_z_mm: None,
            speed_after_spindle: (51.29..51.3).contains(&version),
        };
        let restart = sequence(&state, &options);
        self.job.arc_after_restart = state.motion.filter(|&code| code >= 2);
        self.sled.set_plane(state.plane);
        self.queue_raise(Raise::Restart(Box::new((state, options))));
        Some(restart)
    }

    /// Play: a job paused by the controller or a tool change resumes; otherwise the file starts at the index.
    pub(crate) fn start_run(&mut self, now: Duration, effects: &mut Vec<Effect>) -> Option<String> {
        if self.job.manual_z.is_some() || self.job.flow.in_job() {
            return self.resume_run(now, effects);
        }
        if self.gcode.is_empty() {
            return Some("No GCode file loaded.".into());
        }
        self.park_cancelled();
        self.job.z_target = self.position.z;
        self.sled.file_modes_reset();
        let mut flow = Flow::Streaming;
        if self.index > 0 {
            if let Err(refused) = self.safe_height("starting the file mid-way") {
                return Some(refused);
            }
            let restart = match self.restart_state() {
                Ok(state) => self.queue_restart(state, None, true),
                Err(Unbuilt::XyUnknown) => return Some(XY_UNKNOWN.into()),
                Err(Unbuilt::Unreadable) => None,
            };
            let Some(restart) = restart else {
                self.index = 0;
                return Some("Error with starting run.".into());
            };
            self.push_gcode_position(effects);
            // The firmware pauses on the restart's M6; Resume after the change returns and streams.
            if restart.stopped_at_tool_change {
                flow = Flow::Draining;
            }
        }
        self.job.flow = flow;
        self.leds.light(Led::Play);
        self.queue_played();
        None
    }

    pub(crate) fn pause_run(&mut self, effects: &mut Vec<Effect>) -> Option<String> {
        if self.job.flow != Flow::Streaming {
            self.park_cancelled();
            self.queue_paused(effects);
        }
        if self.job.flow == Flow::Streaming {
            self.job.flow = Flow::Paused;
            effects.push(pause_button(PauseButton::Resume));
            self.job.paused_at = Some((self.index, self.job.z_target));
            self.job.paused_units = Some(self.units());
            self.leds.light(Led::Pause);
        }
        None
    }

    /// Releases the firmware, then, unless it only asked for Z by hand, returns to the file's position at safe height
    /// or higher, and lowers the bit only if it is still the bit that paused there.
    pub(crate) fn resume_run(&mut self, now: Duration, effects: &mut Vec<Effect>) -> Option<String> {
        let tool_change = self.job.manual_z == Some(true);
        // An alarm stopped the firmware, so it no longer waits for Z: the job returns as from any other halt.
        let manual = self.job.manual_z == Some(false) && !self.alarm_held();
        if self.job.flow == Flow::Streaming && !manual {
            // A second Resume must not lift and replunge mid-cut.
            return None;
        }
        let restarts = !manual && self.job.flow.in_job();
        if let Some(Err(refused)) = restarts.then(|| self.safe_height("resuming the job")) {
            return Some(refused);
        }
        let state = match restarts.then(|| self.restart_state()) {
            // Refused before the `~`, so the job stays paused where it is.
            Some(Err(Unbuilt::XyUnknown)) => return Some(XY_UNKNOWN.into()),
            state => state,
        };
        self.job.manual_z = None;
        if tool_change {
            self.queue_tool_fitted();
            // A job's restart returns to its line; the queue goes back to where the sled was as the next file starts.
            if !self.queue_changing_tool() {
                self.park_tool_change_ended();
            }
        }
        self.release_firmware(now, effects);
        if manual {
            self.job.flow = if self.job.flow.in_job() { Flow::Streaming } else { Flow::Idle };
        } else if let Some(mut state) = state {
            if let Some(units) = self.job.paused_units.filter(|&units| units != self.units()) {
                self.follow_units(units == Units::Inches, effects);
                // The state's numbers are in the units it was read in.
                state = self.restart_state();
            }
            let paused_here = self.job.paused_at.filter(|&(index, _)| index == self.index && !tool_change);
            let z_target = paused_here.map(|(_, z)| z);
            if state.ok().and_then(|state| self.queue_restart(state, z_target, false)).is_none() {
                return Some("Error with resuming run.".into());
            }
            self.job.paused_units = None;
            self.job.paused_at = None;
            self.push_gcode_position(effects);
            self.job.flow = Flow::Streaming;
            self.clear_alarm(effects);
        }
        effects.push(pause_button(PauseButton::Pause));
        self.leds.set(Led::Pause, false);
        None
    }

    /// Stop: `!` at once, the queue dropped, and every pause and the job forgotten.
    pub(crate) fn stopped(&mut self, now: Duration, effects: &mut Vec<Effect>) {
        self.queue_stop("Stop was pressed.", effects);
        self.stats.stopping();
        self.probing = false;
        self.measure = None;
        self.pid.end_tests();
        let out_of_step = self.modes_unsure();
        self.realtime("!", now, effects);
        self.controller.stop_reload = true;
        self.drop_user_commands();
        if out_of_step {
            self.restore_modes();
        }
        self.end_job();
        let job = &mut self.job;
        job.manual_z = None;
        job.firmware_paused = false;
        job.holds_job_lines = false;
        // The `!` flushes an M6 the firmware never ran, so it still has the tool from before it.
        if let Some(tool) = job.tool_before_change.take() {
            job.tool = tool;
            job.next_tool = tool;
        }
        job.paused_at = None;
        job.paused_units = None;
    }

    /// Relative mode, or a mode line in flight that the `!` may flush unread, such as a jog's closing G90.
    fn modes_unsure(&self) -> bool {
        let sets_mode = |text: &str| positioning_set_by(text).is_some() || text.contains("G20") || text.contains("G21");
        self.positioning == Positioning::Relative || self.controller.accounting.texts().any(sets_mode)
    }

    /// The firmware gets WebControl's units and G90 again.
    fn restore_modes(&mut self) {
        let units = if self.units() == Units::Inches { "G20 " } else { "G21 " };
        let line = |text: &str| Queued::Line { text: text.into(), boot: true };
        self.controller.queue.extend([line(units), line("G90 ")]);
        self.positioning = Positioning::Absolute;
    }

    pub(crate) fn stop_run(&mut self, now: Duration, effects: &mut Vec<Effect>) -> Option<String> {
        self.stopped(now, effects);
        self.alarm = None;
        self.push_line_position(effects);
        effects.push(broadcast(&ClearAlarm::new()));
        self.leds.light(Led::Stop);
        effects.push(pause_button(PauseButton::Pause));
        None
    }

    /// The link is gone mid-job: the controller's buffered lines are lost, so the job pauses at the oldest of them.
    pub(crate) fn pause_for_lost_link(&mut self, effects: &mut Vec<Effect>) {
        self.rewind_job(effects);
        self.job.flow = Flow::Paused;
        self.job.paused_at = None;
        self.job.paused_units = Some(self.units());
        effects.push(pause_button(PauseButton::Resume));
        self.leds.light(Led::Pause);
    }

    /// `_refuseStreamLine`: the job pauses before a line the controller cannot safely take.
    fn refuse_line(&mut self, alert: (String, String), effects: &mut Vec<Effect>) {
        self.queue_stop(&format!("Line {} was not sent: {}.", self.index + 1, alert.0), effects);
        effects.push(broadcast(&ActivateModal::titled_alert(&alert.0, &alert.1)));
        self.pause_run(effects);
    }

    fn home_shift(&self) -> HomeShift {
        let home = |key: &str| self.settings.number("Advanced Settings", key);
        HomeShift::from_home(home("homeX"), home("homeY"), self.truncate)
    }

    /// A file line as it goes out (home shift, then its arc rewritten), or why it cannot; blank when nothing of it
    /// goes out.
    fn as_written(&self, to_send: &str) -> Result<String, LineProblem> {
        let home = self.home_shift();
        let line = self.sled.convert(&shift_to_home(to_send, self.positioning, &home), [home.x, home.y])?;
        match problem_with(&line).filter(|_| !py::isspace(&line)) {
            Some(problem) => Err(problem),
            None => Ok(line),
        }
    }

    /// A file line with the modal arc a restart left out, if it moves on it; true once the motion is settled.
    fn after_restart(&self, line: String) -> (String, bool) {
        match self.job.arc_after_restart {
            Some(motion) => continue_arc(&line, motion),
            None => (line, false),
        }
    }

    /// The next line as it would go out, or `None` when nothing of it is written.
    fn next_line_written(&self) -> Option<Result<String, LineProblem>> {
        let line = sanitise(self.gcode.get(self.index)?);
        let to_send = self.after_restart(tool_change_line(&line, self.job.next_tool, self.job.tool).line).0;
        let written =
            !line.is_empty() && !py::isspace(&line) && !only_selects_tool(&line) && !reinitialises_axes(&line);
        written.then(|| self.as_written(&to_send)).filter(|line| !line.as_ref().is_ok_and(|line| blank(line)))
    }

    /// Buffered streaming: the next line fits the controller's free bytes, or is refused or skipped without them.
    pub(crate) fn next_line_fits(&self) -> bool {
        match self.next_line_written() {
            Some(Ok(line)) => self.controller.accounting.space() > line.len() as i64,
            _ => true,
        }
    }

    /// `managePause` and `manageToolChange`: the line to send, with streaming stopped after one the firmware pauses on.
    pub(crate) fn manage_tool_change(&mut self, line: &str) -> String {
        let change = tool_change_line(line, self.job.next_tool, self.job.tool);
        self.job.next_tool = change.next_tool;
        if change.pauses {
            self.job.tool_before_change = self.job.tool_before_change.or(Some(self.job.tool));
            self.job.tool = change.next_tool;
            self.job.paused_units = Some(self.units());
            if self.job.flow == Flow::Streaming {
                self.job.flow = Flow::Draining;
            }
        }
        change.line
    }

    fn manage_pause(&mut self, line: &str, effects: &mut Vec<Effect>) {
        if !pauses_firmware(line) {
            return;
        }
        if self.job.flow == Flow::Streaming {
            self.job.flow = Flow::Draining;
        }
        self.job.paused_units = Some(self.units());
        effects.push(pause_button(PauseButton::Resume));
    }

    /// `sendNextLine`: one file line, or the pause before one the controller cannot take.
    pub(crate) fn send_next_line(&mut self, now: Duration, effects: &mut Vec<Effect>) {
        let Some(raw) = self.gcode.get(self.index) else {
            return self.end_job();
        };
        let line = self.queue_skip_bit_change(sanitise(raw), effects);
        let (to_send, settled) = self.after_restart(tool_change_line(&line, self.job.next_tool, self.job.tool).line);
        let shifted = match self.as_written(&to_send) {
            Ok(shifted) => shifted,
            Err(problem) => return self.refuse_line(problem.alert(self.index), effects),
        };
        if settled {
            self.job.arc_after_restart = None;
        }
        self.sled.sent(&to_send);
        self.manage_pause(&line, effects);
        let line = self.manage_tool_change(&line);
        if reinitialises_axes(&line) {
            effects.push(alert(&format!("Line {} {SKIPPED_REINIT}", self.index + 1)));
        } else if !blank(&line) && !blank(&shifted) && !only_selects_tool(&line) {
            let positioning = positioning_set_by(&line).unwrap_or(self.positioning);
            let id = self.write(&shifted, Some(self.index), now, effects);
            self.track_probe(&shifted, id, Some(self.index));
            self.after_line_sent(&shifted, positioning, effects);
        }
        if self.index + 1 < self.gcode.len() {
            self.index += 1;
        } else {
            if !self.queue_ran_out() {
                self.park_when_finished(crate::park::Trigger::File);
            }
            self.end_job();
        }
    }

    /// `_afterLineSent`: the Z target, the units a G20/G21 switches to, and the line's position for the viewer.
    fn after_line_sent(&mut self, line: &str, positioning: Positioning, effects: &mut Vec<Effect>) {
        let Ok(z) = word(line, 'Z') else {
            return tracing::debug!(line, "unreadable Z word; Python raised here");
        };
        // A probe's Z is a limit, not a target; its answer sets Z.
        if let Some(z) = z.filter(|_| !line.contains("G38.2")) {
            self.job.z_target = if positioning == Positioning::Absolute { z } else { self.job.z_target + z };
        }
        self.follow_units_in(line, effects);
        self.push_line_position(effects);
    }

    /// A G20 or G21 sent to the controller switches WebControl's units with it.
    pub(crate) fn follow_units_in(&mut self, line: &str, effects: &mut Vec<Effect>) {
        if line.contains("G20") && self.units() != Units::Inches {
            self.follow_units(true, effects);
        }
        if line.contains("G21") && self.units() != Units::Mm {
            self.follow_units(false, effects);
        }
    }

    /// `sendGCodePositionUpdate` without recalculating: the line at the index, from the one streamed before it.
    pub(crate) fn push_line_position(&mut self, effects: &mut Vec<Effect>) {
        let Some(raw) = self.gcode.get(self.index) else { return };
        let Ok(target) = line_target(self.job.streamed, self.positioning, &sanitise(raw)) else { return };
        self.job.streamed = target;
        let scale = match (self.preview.file_units, self.units()) {
            (Units::Mm, Units::Inches) => 1.0 / 25.4,
            (Units::Inches, Units::Mm) => 25.4,
            _ => 1.0,
        };
        effects.push(broadcast(&GcodePositionMessage {
            xval: target.x * scale,
            yval: target.y * scale,
            zval: target.z * scale,
            gcode_line: raw.clone(),
            gcode_line_index: self.index,
        }));
    }
}
