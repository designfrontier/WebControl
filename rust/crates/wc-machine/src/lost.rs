//! A line the controller never received (`spec/firmware.md` B18). The firmware prints a line's echo as it takes
//! the line from its ring, before running it, and reports the ring's free space with every status report, so a line
//! with no echo, no `ok`, an empty ring and a still sled never arrived and is safe to send again.

use std::time::Duration;

use wc_firmware::{EMPTY_RING, Reply, RunMode, Sent};
use wc_protocol::main::{ActivateModal, ControllerMessage, PauseButton, Units};

use crate::gpio::Led;
use crate::job::{Flow, pause_button};
use crate::machine::{Effect, Machine, broadcast};

/// The firmware reads serial into its ring before every report and a line is under 10 ms on the wire, so a later
/// report shows the line in the ring or follows its echo (the owner's 51.27 echoes about 20 ms after the write).
pub const GRACE: Duration = Duration::from_millis(150);

/// Reports past `GRACE` in a row, so one garbled or glued report is never enough.
pub const LOST_AFTER: u32 = 3;

const NOT_RECEIVED: &str = "Line Not Received";

/// What the controller has said since the last ack, as evidence that the line it waits for never arrived.
#[derive(Debug, Clone, Default)]
pub(crate) struct Watch {
    /// The firmware printed something other than a status report, so it has taken a line.
    heard: bool,
    /// MPos in the latest status report.
    still_at: Option<(f64, f64, f64)>,
    /// When the latest line went out.
    written_at: Option<Duration>,
    /// Reports past `GRACE` in a row with an empty ring and a still sled.
    empty: u32,
    /// A line went out while another waited for its ack, so an `ok` may belong to a line it is not counted for.
    crowded: bool,
    /// Lines sent again once, by the id they went out with the second time.
    resent: Vec<u64>,
}

impl Watch {
    /// A write starts the wait over; what the controller said before a write to an idle controller was about
    /// lines already answered.
    pub fn written(&mut self, idle: bool, now: Duration) {
        self.empty = 0;
        self.written_at = Some(now);
        self.heard &= !idle;
        self.crowded = !idle;
    }

    pub fn realtime_written(&mut self) {
        self.empty = 0;
    }

    /// A `~`'s ack says nothing about the line the firmware runs.
    pub fn acked(&mut self, sent: Option<&Sent>) {
        self.empty = 0;
        if let Some(sent) = sent.filter(|sent| sent.text.is_some()) {
            self.heard = false;
            self.resent.retain(|&id| id != sent.id);
        }
    }

    /// True once `LOST_AFTER` reports in a row say the unacknowledged lines never reached the controller.
    pub fn read(&mut self, reply: &Reply, now: Duration) -> bool {
        match reply {
            Reply::Ok | Reply::Error(_) | Reply::Misread(_) => self.empty = 0,
            Reply::Status(status) => {
                let at = Some((status.x, status.y, status.z));
                if status.mode != RunMode::Idle || at != self.still_at {
                    self.empty = 0;
                }
                self.still_at = at;
            }
            Reply::PositionError(error) if error.space == EMPTY_RING && self.still_at.is_some() => {
                if self.written_at.is_none_or(|at| now.saturating_sub(at) >= GRACE) {
                    self.empty = self.empty.saturating_add(1);
                }
                return !self.heard && self.empty >= LOST_AFTER;
            }
            Reply::PositionError(_) => self.empty = 0,
            _ => {
                self.heard = true;
                self.empty = 0;
            }
        }
        false
    }
}

fn shown(sent: &Sent) -> &str {
    sent.text.as_deref().unwrap_or_default().trim()
}

impl Machine {
    /// Nothing else explains the silence: no boot, resync, firmware pause, `~` waiting for its ack, or line that
    /// went out behind another.
    fn lost_lines(&self) -> Option<Vec<Sent>> {
        let controller = &self.controller;
        let quiet = controller.resync_after.is_none()
            && !controller.boot_in_progress
            && controller.boot_deadline.is_none()
            && !self.job.firmware_paused
            && self.job.manual_z.is_none()
            && !controller.watch.crowded;
        let lines = controller.accounting.lines().filter(|lines| quiet && !lines.is_empty())?;
        Some(lines.into_iter().cloned().collect())
    }

    pub(crate) fn watch_for_lost_line(&mut self, reply: &Reply, now: Duration, effects: &mut Vec<Effect>) {
        if !self.controller.watch.read(reply, now) {
            return;
        }
        let Some(lost) = self.lost_lines() else { return };
        tracing::warn!(lines = lost.len(), "the controller never received the lines it was sent");
        self.controller.accounting.take();
        self.controller.ready = true;
        let again = lost.iter().any(|sent| self.controller.watch.resent.contains(&sent.id));
        self.controller.watch.resent.clear();
        let index = lost.iter().find_map(|sent| sent.index);
        let held = matches!(self.job.flow, Flow::Paused | Flow::Held);
        self.pacing_lost(&lost, !again && !held, again, now);
        match index {
            Some(index) if again => {
                let message = format!(
                    "The controller did not receive line {} ({}), even when it was sent again. The job is paused \
                     at that line. Press Resume to send it again, or Stop.",
                    index + 1,
                    shown(&lost[0])
                );
                effects.push(broadcast(&ActivateModal::titled_alert(NOT_RECEIVED, &message)));
                self.hold_at_lost_line(index, effects);
            }
            Some(index) if held => {
                let note = format!("Line {} did not reach the controller; Resume sends it again.", index + 1);
                effects.push(broadcast(&ControllerMessage(note)));
                self.hold_at_lost_line(index, effects);
            }
            None if again => self.drop_lost_command(&lost[0], effects),
            _ => lost.iter().for_each(|sent| self.send_again(sent, now, effects)),
        }
    }

    /// The same bytes again; WebControl already followed what they change when it first wrote them.
    fn send_again(&mut self, sent: &Sent, now: Duration, effects: &mut Vec<Effect>) {
        let text = sent.text.clone().unwrap_or_default();
        let note = match sent.index {
            Some(index) => {
                format!("Line {} did not reach the controller, so it was sent again: {}", index + 1, shown(sent))
            }
            None => format!("The controller did not receive {}, so it was sent again.", shown(sent)),
        };
        effects.push(broadcast(&ControllerMessage(note)));
        let idle = self.controller.accounting.is_empty();
        let id = self.controller.accounting.sent(&text, sent.index);
        self.controller.watch.resent.push(id);
        self.controller.watch.written(idle, now);
        self.controller.ready = false;
        self.controller.last_write = Some(now);
        self.controller.paced_until = None;
        if let Some(probe) = self.controller.probe.as_mut().filter(|probe| probe.id == sent.id) {
            probe.id = id;
        }
        effects.push(Effect::Write(text.as_bytes().to_vec()));
        effects.push(Effect::Log(format!("Sent: {}", text.trim_end_matches('\n'))));
    }

    /// The job waits at the line, as after a lost link; Resume returns to the file there and sends it.
    fn hold_at_lost_line(&mut self, index: usize, effects: &mut Vec<Effect>) {
        self.queue_stop("A line did not reach the controller.", effects);
        self.index = index;
        // An M6 the firmware never received changed no tool.
        if let Some(tool) = self.job.tool_before_change.take() {
            self.job.tool = tool;
        }
        if !matches!(self.job.flow, Flow::Paused | Flow::Held) {
            self.job.flow = Flow::Paused;
        }
        self.job.paused_at = None;
        self.job.paused_units = Some(self.units());
        self.forget_lost_moves();
        self.push_gcode_position(effects);
        effects.push(pause_button(PauseButton::Resume));
        self.leds.light(Led::Pause);
    }

    /// A command lost twice is dropped with what was queued after it; the firmware gets WebControl's units and G90.
    fn drop_lost_command(&mut self, sent: &Sent, effects: &mut Vec<Effect>) {
        self.queue_stop("A command did not reach the controller.", effects);
        let mut message = format!(
            "The controller did not receive {}, even when it was sent again, so it and the commands queued after it \
             were dropped.",
            shown(sent)
        );
        self.drop_user_commands();
        self.forget_lost_moves();
        let units = if self.units() == Units::Inches { "G20 " } else { "G21 " };
        self.queue([units.to_string(), "G90 ".to_string()]);
        if self.job.flow.in_job() {
            self.job.flow = Flow::Paused;
            self.job.paused_at = None;
            self.job.paused_units = Some(self.units());
            effects.push(pause_button(PauseButton::Resume));
            self.leds.light(Led::Pause);
            message.push_str(" The job is paused. Press Resume to return to it, or Stop.");
        }
        effects.push(broadcast(&ActivateModal::titled_alert(NOT_RECEIVED, &message)));
    }

    /// WebControl followed the lost lines' Z, XY and probe as if they ran.
    fn forget_lost_moves(&mut self) {
        self.controller.probe = None;
        self.probing = false;
        self.bit.lost();
        self.sled.lost();
    }
}
