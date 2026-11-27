//! B2: what a boot writes once `$$` has said what the controller holds, and the restart a re-initialising write
//! (`$38`, which re-runs `setupAxes()` on the live board) needs before the motors follow again.

use std::time::Duration;

use wc_firmware::boot::firmware_build;
use wc_firmware::settings::{
    Correction, WRITE_ONLY_KEY, adopted_text, boot_push, boot_sync, corrections, reinitialises_axes, setting_key,
    skip_held_reinit_keys,
};
use wc_firmware::upgrade::migration_push;

use wc_protocol::main::ControllerMessage;

use crate::machine::{Effect, Link, Machine, Queued, alert, broadcast};

const EEPROM_RESTORED: &str = "The controller lost its settings (EEPROM read fail), so WebControl restored them. \
                               WebControl doesn't keep Z-axis limits, so set them again if you use them.";
const RESTARTING: &str = "WebControl is restarting the controller so its new chain-over-sprocket setting ($38) takes \
                          effect: the firmware re-initialises its motors when that setting is written, and they do \
                          not move again until it reboots. Wait for it to reconnect before moving.";
const NOT_RESTARTED: &str = "The controller did not restart after its chain-over-sprocket setting ($38) was written, \
                             so its motors will not follow. Power-cycle the controller or press its reset button; \
                             WebControl refuses moves until it boots.";
const STILL_DIFFERS: &str = "The controller still reports a different chain-over-sprocket setting ($38) after \
                             restarting, so WebControl did not write it again. Check Advanced Settings, then restart \
                             the controller.";
const PID_REPAIRED: &str = "The controller held PID values its firmware ignores, which leave the motors unable to \
                            move, so WebControl wrote its own:";
const JOB_HOLDS: &str = "The chain-over-sprocket setting ($38) was not sent: writing it re-initialises the motors, \
                         so the controller has to restart. Stop the job first.";

/// Where the reset after a re-initialising write has got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Restart {
    None,
    /// The port was closed so that reopening it resets the board.
    Pending,
    /// The port reopened without a boot banner: the board did not reset.
    Missed,
    /// It booted; its sync writes no re-initialising key again, so a value that will not stick cannot loop.
    Booted,
}

impl Machine {
    pub(crate) fn expand_settings_sync(&mut self, effects: &mut Vec<Effect>) {
        let eeprom_reset = std::mem::take(&mut self.controller.eeprom_reset);
        let reported = std::mem::take(&mut self.controller.reported);
        let (settings, firmware) = (&self.settings, self.firmware_version);
        let migration = self.upgrade.migrating();
        let build = firmware_build(firmware, self.rewrite_notice);
        let mut push = match (migration, eeprom_reset) {
            (Some(migration), _) => migration_push(settings, firmware, build.as_deref(), migration),
            (None, true) => boot_push(settings, firmware, true),
            (None, false) => boot_sync(settings, firmware, &reported),
        };
        skip_held_reinit_keys(&mut push, &reported);
        if self.restart != Restart::None && push.lines.iter().any(|line| reinitialises_axes(line)) {
            push.lines.retain(|line| !reinitialises_axes(line));
            self.controller.reinit_suppressed = true;
            effects.push(alert(STILL_DIFFERS));
        }
        if self.restart == Restart::Booted {
            self.restart = Restart::None;
        }
        if migration.is_none() && !eeprom_reset {
            self.announce_writes(&push.lines, &reported, effects);
        }
        push.adopted.into_iter().for_each(|(section, key, value)| self.set_setting(&section, &key, value));
        // `$13` alone needs no read-back: the controller already reported it.
        let verify = push.lines.iter().any(|line| setting_key(line) != Some(WRITE_ONLY_KEY));
        let read_back = verify.then(|| "$$".to_string());
        push.lines
            .into_iter()
            .chain(read_back)
            .rev()
            .for_each(|text| self.controller.queue.push_front(Queued::Line { text, boot: true }));
        if eeprom_reset && migration.is_none() {
            effects.push(alert(EEPROM_RESTORED));
        }
    }

    /// Tells the user what each write changes on the controller, with an alert for PID values it repairs.
    pub(crate) fn announce_writes(&self, lines: &[String], reported: &[(u32, f64)], effects: &mut Vec<Effect>) {
        let corrections = corrections(&self.settings, self.firmware_version, lines, reported);
        let change = |c: &Correction| format!("${} ({}) was {}", c.key, c.setting, adopted_text(c.controller));
        corrections.iter().for_each(|c| {
            let text = format!("Settings sync: {} on the controller; WebControl wrote {}.", change(c), c.written);
            tracing::info!("{text}");
            effects.push(broadcast(&ControllerMessage(text)));
        });
        let repairs: Vec<String> =
            corrections.iter().filter(|c| c.repair).map(|c| format!("{}, now {}.", change(c), c.written)).collect();
        if !repairs.is_empty() {
            effects.push(alert(&format!("{PID_REPAIRED} {}", repairs.join(" "))));
        }
    }

    /// A re-initialising line went out and every line before the next non-setting one is answered.
    pub(crate) fn restart_due(&self) -> bool {
        let next_is_setting =
            matches!(self.controller.queue.front(), Some(Queued::Line { text, .. }) if setting_key(text).is_some());
        self.controller.axes_reinitialised && !next_is_setting
    }

    /// Reopening the port drops DTR, which resets the Mega; the queue behind the write goes with the old link.
    pub(crate) fn restart_controller(&mut self, now: Duration, effects: &mut Vec<Effect>) {
        tracing::warn!("restarting the controller after a re-initialising setting write");
        self.restart = Restart::Pending;
        self.probing = false;
        self.bit.lost();
        let port = match std::mem::replace(&mut self.link, Link::Disconnected { retry_at: now }) {
            Link::Connected { port } => port,
            _ => return,
        };
        effects.push(Effect::ClosePort);
        effects.push(broadcast(&self.connection_status()));
        effects.push(alert(RESTARTING));
        // The same port, whatever `COMport` says now; reopening it is what resets the board.
        effects.push(Effect::OpenPort(port, self.port_baud()));
        self.link = Link::Opening;
    }

    /// A handshake that starts without a boot banner after a restart means the board never reset.
    pub(crate) fn restart_not_seen(&mut self, effects: &mut Vec<Effect>) {
        if self.restart == Restart::Pending {
            self.restart = Restart::Missed;
            effects.push(alert(NOT_RESTARTED));
        }
    }

    pub(crate) fn restart_booted(&mut self) {
        if matches!(self.restart, Restart::Pending | Restart::Missed) {
            self.restart = Restart::Booted;
        }
    }

    /// Why a command that may move the machine waits for the restart.
    pub(crate) fn restart_refusal(&self) -> Option<&'static str> {
        match self.restart {
            Restart::Pending => Some(RESTARTING),
            Restart::Missed => Some(NOT_RESTARTED),
            Restart::None | Restart::Booted => None,
        }
    }

    /// False, with an alert, for a re-initialising line while a job holds the machine.
    pub(crate) fn may_reinitialise(&self, line: &str, effects: &mut Vec<Effect>) -> bool {
        if !reinitialises_axes(line) || !self.gcode_locked() {
            return true;
        }
        effects.push(alert(JOB_HOLDS));
        false
    }
}
