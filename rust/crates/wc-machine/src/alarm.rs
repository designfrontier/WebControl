//! The alarm hold: after the controller halts itself (`ALARM:`, `Buffer overflow!`) or cannot place the sled, nothing
//! that moves the machine is written until Stop, or Play on the job the alarm held, clears it.

use wc_gcode::sanitise::sanitise;
use wc_protocol::main::{Alarm, ClearAlarm};

use crate::machine::{Effect, Machine, Queued, broadcast};

pub(crate) const HELD: &str = "The controller raised an alarm and halted. Press Stop to clear it before sending it \
                               anything else.";
pub(crate) const JOB_HELD: &str = "The controller raised an alarm and halted the job. Press Stop to clear it, or Play \
                                   to resume the job from the line it stopped at.";

/// Actions that move nothing: Stop, Pause, `~`, `$$` and what never reaches the controller.
const ALLOWED: [&str; 21] = [
    "stopRun",
    "stopZ",
    "pauseRun",
    "resumeRun",
    "statusRequest",
    "toggleCamera",
    "queryCamera",
    "shutdown",
    "TurnOffRPI",
    "updatePorts",
    "clearLogs",
    "createDirectory",
    "boardProcessGCode",
    "boardClearBoard",
    "defineHome",
    "resetHomeToCenter",
    "clearGCode",
    "moveGcodeZ",
    "moveGcodeGoto",
    "moveGcodeIndex",
    "reportSettings",
];

/// Settings and modal lines that never move the machine; the firmware takes `$` lines as system commands.
/// A lone `G91` is left out: a later absolute move would run relative.
pub(crate) fn moves_nothing(text: &str) -> bool {
    let line = sanitise(text);
    let words: Vec<&str> = line.split_whitespace().collect();
    line.trim_start().starts_with('$') || matches!(words[..], ["G20" | "G21" | "G90" | "B05"])
}

/// The boot's handshake and settings, the upgrade's marks, and lines that move nothing.
pub(crate) fn survives_alarm(queued: &Queued) -> bool {
    match queued {
        Queued::Line { boot: true, .. } | Queued::SettingsPush | Queued::Mark(_) => true,
        Queued::Line { text, .. } => moves_nothing(text),
        Queued::Delay(_) | Queued::Raise(_) => false,
    }
}

impl Machine {
    pub(crate) fn alarm_held(&self) -> bool {
        self.alarm.is_some()
    }

    /// Why `command` is refused while an alarm holds the controller; Play on the held job is how it resumes.
    pub(crate) fn alarm_refusal(&self, command: &str) -> Option<&'static str> {
        let resumes_job = matches!(command, "startRun" | "resumeRun") && self.job.flow.in_job();
        if !self.alarm_held() || resumes_job || ALLOWED.contains(&command) {
            return None;
        }
        Some(if self.job.flow.in_job() { JOB_HELD } else { HELD })
    }

    /// Drops every queued command that would move the machine and holds the rest until the alarm is cleared.
    pub(crate) fn latch_alarm(&mut self, message: String, effects: &mut Vec<Effect>) {
        tracing::warn!(message, "alarm: holding every command that moves the machine until Stop");
        self.controller.queue.retain(survives_alarm);
        self.controller.hold_until = None;
        self.controller.raise_deadline = None;
        effects.push(broadcast(&Alarm { message: message.clone() }));
        self.alarm = Some(message);
    }

    /// Stop, or Play on the held job: the UI's alarm goes with it.
    pub(crate) fn clear_alarm(&mut self, effects: &mut Vec<Effect>) {
        if self.alarm.take().is_some() {
            tracing::info!("alarm cleared");
            effects.push(broadcast(&ClearAlarm::new()));
        }
    }

    /// What a B05 reply shows: no alarm after a reconnect, unless one still holds the controller.
    pub(crate) fn alarm_after_boot(&self) -> Effect {
        match &self.alarm {
            Some(message) => broadcast(&Alarm { message: message.clone() }),
            None => broadcast(&ClearAlarm::new()),
        }
    }
}
