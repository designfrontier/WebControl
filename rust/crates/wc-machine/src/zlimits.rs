//! Z travel limits (upstream #136): Z coordinates in mm, moved with every redefinition of Z (Define Z0, Touch Z, a
//! G10) so they stay at one place on the Z mechanism. A blank limit is no limit.

use serde_json::Value;
use wc_config::python_float_str;
use wc_gcode::Positioning;
use wc_gcode::commanded_z::ZChange;
use wc_gcode::sanitise::LineProblem;
use wc_protocol::float;
use wc_protocol::main::PauseButton;

use crate::commands::round4;
use crate::gpio::Led;
use crate::job::{Flow, pause_button};
use crate::machine::{Effect, Machine};

const SECTION: &str = "Maslow Settings";
pub const UPPER_KEY: &str = "zAxisUpperLimit";
pub const LOWER_KEY: &str = "zAxisLowerLimit";
/// A Z exactly at a limit, converted from inches, must not be refused.
const TOLERANCE_MM: f64 = 1e-6;

impl Machine {
    fn z_limit(&self, key: &str) -> Option<f64> {
        self.settings.value(SECTION, key).and_then(float)
    }

    pub(crate) fn z_lower_limit_mm(&self) -> Option<f64> {
        self.z_limit(LOWER_KEY)
    }

    /// Why `line`, written next, would take Z past a limit.
    pub(crate) fn z_limit_problem(&self, line: &str) -> Option<LineProblem> {
        let (lower, upper) = (self.z_limit(LOWER_KEY), self.z_limit(UPPER_KEY));
        if lower.is_none() && upper.is_none() {
            return None;
        }
        // A probe's touch, and a G10, rename Z from where the bit is, which must be known to move the limits.
        let start_needed = line.contains("G38.2") && self.bit.mm().is_none();
        match self.bit.change(line) {
            ZChange::Unchanged => None,
            ZChange::Renamed(to) => (to.is_none() || self.bit.mm().is_none()).then_some(LineProblem::ZUnknown),
            ZChange::MovesTo(None) => Some(LineProblem::ZUnknown),
            ZChange::MovesTo(Some(_)) if start_needed => Some(LineProblem::ZUnknown),
            ZChange::MovesTo(Some(z_mm)) => {
                let below = lower.filter(|&limit| z_mm < limit - TOLERANCE_MM).map(|limit| (limit, false));
                let above = upper.filter(|&limit| z_mm > limit + TOLERANCE_MM).map(|limit| (limit, true));
                below.or(above).map(|(limit_mm, upper)| LineProblem::ZLimit { z_mm, limit_mm, upper })
            }
        }
    }

    /// Before `line` is written: a G10 renames the bit's Z, and the limits move with it.
    pub(crate) fn follow_z_rename(&mut self, line: &str) {
        if let ZChange::Renamed(Some(to)) = self.bit.change(line)
            && let Some(from) = self.bit.mm()
        {
            self.shift_z_limits(to - from);
        }
    }

    /// What was queued after a refused command may rely on it (a raise before travel), so it is dropped, as by Stop;
    /// a restart's job pauses.
    pub(crate) fn drop_after_z_refusal(&mut self, effects: &mut Vec<Effect>) {
        self.drop_user_commands();
        if self.positioning == Positioning::Relative {
            self.queue(["G90 ".to_string()]);
        }
        if matches!(self.job.flow, Flow::Streaming | Flow::Draining) {
            self.queue_stop("A command would have moved Z past a limit.", effects);
            self.job.flow = Flow::Paused;
            self.job.paused_at = None;
            self.job.paused_units = Some(self.units());
            effects.push(pause_button(PauseButton::Resume));
            self.leds.light(Led::Pause);
        }
    }

    pub(crate) fn shift_z_limits(&mut self, by_mm: f64) {
        [UPPER_KEY, LOWER_KEY].into_iter().for_each(|key| {
            if let Some(limit) = self.z_limit(key) {
                self.set_setting(SECTION, key, Value::from(python_float_str(round4(limit + by_mm))));
            }
        });
    }
}
