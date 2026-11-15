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

#[derive(Clone, Debug)]
pub(crate) struct Hold {
    message: String,
    /// False once the firmware could not place the sled: it then takes the sled to be at 0, 0.
    job_resumes: bool,
}

impl Machine {
    pub(crate) fn alarm_held(&self) -> bool {
        self.alarm.is_some()
    }

    fn job_resumes(&self) -> bool {
        self.job.flow.in_job() && self.alarm.as_ref().is_some_and(|hold| hold.job_resumes)
    }

    /// Why `command` is refused while an alarm holds the controller; Play on the held job is how it resumes.
    pub(crate) fn alarm_refusal(&self, command: &str) -> Option<&'static str> {
        let play = matches!(command, "startRun" | "resumeRun") && self.job.flow.in_job();
        let allowed = if play { self.job_resumes() } else { ALLOWED.contains(&command) };
        if !self.alarm_held() || allowed {
            return None;
        }
        Some(if self.job_resumes() { JOB_HELD } else { HELD })
    }

    /// Drops every queued command that would move the machine and holds the rest until the alarm is cleared.
    pub(crate) fn latch_alarm(&mut self, message: String, job_resumes: bool, effects: &mut Vec<Effect>) {
        tracing::warn!(message, "alarm: holding every command that moves the machine until Stop");
        self.controller.queue.retain(survives_alarm);
        self.controller.hold_until = None;
        self.controller.raise_deadline = None;
        effects.push(broadcast(&Alarm { message: message.clone() }));
        let job_resumes = job_resumes && self.alarm.as_ref().is_none_or(|hold| hold.job_resumes);
        self.alarm = Some(Hold { message, job_resumes });
    }

    /// `Unable to find valid machine position`: Stop's own reload repeats it, so only that one does not hold.
    pub(crate) fn unplaceable_sled(&mut self, message: String, effects: &mut Vec<Effect>) {
        match std::mem::take(&mut self.controller.stop_reload) {
            true => effects.push(broadcast(&Alarm { message })),
            false => self.latch_alarm(message, false, effects),
        }
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
            Some(hold) => broadcast(&Alarm { message: hold.message.clone() }),
            None => broadcast(&ClearAlarm::new()),
        }
    }
}
