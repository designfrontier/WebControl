//! Z travel limits (upstream #136): 51.292's `$63`/`$64`, which the firmware moves with every change of Z zero and
//! enforces by clamping Z there. WebControl mirrors them to refuse a move past one with an alert instead.

use wc_gcode::Positioning;
use wc_gcode::commanded_z::ZChange;
use wc_gcode::sanitise::LineProblem;
use wc_gcode::shift::positioning_set_by;
use wc_protocol::main::{PauseButton, Units};

use crate::gpio::Led;
use crate::job::{Flow, pause_button};
use crate::machine::{Effect, Machine, Queued};

/// `$$` prints 8 decimals; a Z exactly at a limit, converted from inches, must not be refused.
const EXACT_MM: f64 = 1e-6;
/// The other replies print 2 decimals, so the firmware's limit may be this far past the printed one.
const PRINTED: f64 = 0.005;
const B20_UPPER: &str = "Z Axis Upper Limit: ";
const B20_LOWER: &str = " Lower Limit: ";

/// The firmware's limits in mm in its current Z, each with how far it may be past what was printed; `None` while
/// unknown or none.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Mirror {
    /// The firmware has reported limits, so it keeps them.
    kept: bool,
    upper: Option<(f64, f64)>,
    lower: Option<(f64, f64)>,
}

impl Machine {
    /// Why `line`, written next, would take Z past a limit. Where WebControl cannot follow Z (a probe, which the
    /// firmware stops at the limit, among them), the firmware's clamp still holds.
    pub(crate) fn z_limit_problem(&self, line: &str) -> Option<LineProblem> {
        let ZChange::MovesTo(Some(z_mm)) = self.bit.change(line) else { return None };
        let Mirror { upper, lower, .. } = self.controller.z_limits;
        let below = lower.filter(|&(limit, slack)| z_mm < limit - slack).map(|(limit, _)| (limit, false));
        let above = upper.filter(|&(limit, slack)| z_mm > limit + slack).map(|(limit, _)| (limit, true));
        below.or(above).map(|(limit_mm, upper)| LineProblem::ZLimit { z_mm, limit_mm, upper })
    }

    /// `$63`/`$64` as `$$` lists them, in mm.
    pub(crate) fn z_limit_setting(&mut self, key: u32, value: f64) {
        self.z_limit_reported(key, value, EXACT_MM);
    }

    fn z_limit_reported(&mut self, key: u32, value: f64, slack_mm: f64) {
        let limits = &mut self.controller.z_limits;
        let limit = value.is_finite().then_some((value, slack_mm));
        match key {
            63 => limits.upper = limit,
            64 => limits.lower = limit,
            _ => return,
        }
        limits.kept = true;
    }

    /// The reply to a `$63`/`$64` write (mm), or to B20 (the controller's units).
    pub(crate) fn z_limit_line(&mut self, line: &str) {
        let number = |text: &str| text.trim().parse::<f64>().ok();
        if let Some(value) = line.strip_prefix("new zaxis upper limit=").and_then(number) {
            return self.z_limit_reported(63, value, PRINTED);
        }
        if let Some(value) = line.strip_prefix("new zaxis lower limit=").and_then(number) {
            return self.z_limit_reported(64, value, PRINTED);
        }
        let Some((upper, lower)) = line.strip_prefix(B20_UPPER).and_then(|rest| rest.split_once(B20_LOWER)) else {
            return;
        };
        let Some(mm_per_unit) = self.bit.units().map(|units| if units == Units::Inches { 25.4 } else { 1.0 }) else {
            return;
        };
        // B20 prints NAN for no limit, which parses as NaN.
        let mm = |text: &str| number(text).map_or(f64::NAN, |value| value * mm_per_unit);
        self.z_limit_reported(63, mm(upper), PRINTED * mm_per_unit);
        self.z_limit_reported(64, mm(lower), PRINTED * mm_per_unit);
    }

    /// A job streams no further while a B20 is unanswered, so no line goes out unchecked after a change of zero.
    pub(crate) fn z_limits_awaited(&self) -> bool {
        self.controller.accounting.texts().any(|text| text.starts_with("B20"))
    }

    /// Before `line` is written: a G10 moves the firmware's limits by an amount only it knows.
    pub(crate) fn follow_z_rename(&mut self, line: &str) {
        if matches!(self.bit.change(line), ZChange::Renamed(_)) {
            self.z_zero_changed();
        }
    }

    /// The firmware moved its limits with the zero (a G10, a touch, a manual Z adjust), so they are read again.
    pub(crate) fn z_zero_changed(&mut self) {
        let limits = &mut self.controller.z_limits;
        if !limits.kept {
            return;
        }
        (limits.upper, limits.lower) = (None, None);
        self.controller.queue.push_front(Queued::Line { text: "B20 ".into(), boot: true });
    }

    /// What was queued after a refused command may rely on it (a raise before travel), so it is dropped, as by Stop;
    /// a restart's job pauses.
    pub(crate) fn drop_after_z_refusal(&mut self, effects: &mut Vec<Effect>) {
        let restores_absolute = self.controller.queue.iter().any(|queued| {
            matches!(queued, Queued::Line { text, boot: false } if positioning_set_by(text) == Some(Positioning::Absolute))
        });
        self.drop_user_commands();
        if self.positioning == Positioning::Relative && restores_absolute {
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
}
