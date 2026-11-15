//! Owner decision (2026-11-15): before any XY travel that starts a resume, a restart or a manual return, Z goes to the
//! higher of the safe height and the bit's own height, so nothing before the travel lowers the bit.

use std::time::Duration;

use wc_config::python_float_str;
use wc_gcode::commanded_z::CommandedZ;
use wc_gcode::restart::{GcodeState, Options, round_up, sequence};
use wc_protocol::main::{ActivateModal, PauseButton, Units};

use crate::gpio::Led;
use crate::job::{Flow, pause_button};
use crate::machine::{Effect, Machine, Queued, broadcast};

/// The firmware reports about every 200 ms (§2 of spec/firmware.md), so this is many missed reports, yet less than
/// the link timeout.
pub const HEIGHT_WAIT: Duration = Duration::from_secs(3);
const HEIGHT_UNKNOWN_TITLE: &str = "Bit Height Unknown";
const HEIGHT_UNKNOWN: &str = "The sled did not move: WebControl could not tell how high the bit is, because the \
                              controller sent no status report for 3 seconds. Check the controller's connection, then \
                              try again.";

/// Where the bit is: the Z WebControl commanded, or else the last status report's, one display step higher.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BitHeight {
    commanded: CommandedZ,
    /// MPos z read since the last line that moved Z (or changed units) was acknowledged, in mm.
    reported_mm: Option<f64>,
    /// Such a line has not been acknowledged yet, so a report may show the bit mid-move.
    moving: bool,
}

impl BitHeight {
    pub fn new() -> BitHeight {
        BitHeight { commanded: CommandedZ::new(), reported_mm: None, moving: false }
    }

    /// Every acknowledged line before a raise has finished, so the commanded Z is where the bit is.
    pub fn mm(&self) -> Option<f64> {
        self.commanded.z_mm().or(self.reported_mm)
    }

    pub fn written(&mut self, line: &str) {
        if self.commanded.z_mm().is_none() {
            // A relative move from an unknown height starts at the report, an upper bound.
            self.commanded.set_z_mm(self.reported_mm);
        }
        if self.commanded.written(line) {
            self.reported_mm = None;
            self.moving = true;
        }
    }

    /// Every line written has been acknowledged.
    pub fn settled(&mut self) {
        self.moving = false;
    }

    /// MPos has two decimals, so the bit may be up to one step above it.
    pub fn reported(&mut self, z: f64) {
        let Some(units) = self.commanded.units().filter(|_| !self.moving) else { return };
        let mm_per_unit = if units == Units::Inches { 25.4 } else { 1.0 };
        self.reported_mm = z.is_finite().then_some((z + 0.01) * mm_per_unit);
    }

    /// A Stop, an alarm, a probe or a reboot left the bit somewhere a status report must tell.
    pub fn lost(&mut self) {
        self.commanded.set_z_mm(None);
        self.reported_mm = None;
        self.moving = false;
    }

    pub fn zeroed(&mut self) {
        self.commanded.set_z_mm(Some(0.0));
        self.reported_mm = None;
    }
}

/// A raise whose height is decided when it reaches the controller, after every line queued before it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Raise {
    /// Home and Return to Center, in the machine's units.
    Manual {
        units: Units,
        safe_height_mm: f64,
    },
    /// Triangular Accept, after its `G21`.
    Accept {
        safe_height_mm: f64,
    },
    Restart(Box<(GcodeState, Options)>),
}

impl Raise {
    pub fn lines(&self, bit_mm: Option<f64>) -> Vec<String> {
        let above = |safe_height_mm: f64| bit_mm.filter(|&z| z > safe_height_mm);
        match self {
            Raise::Manual { units: Units::Inches, safe_height_mm } => {
                let z = above(*safe_height_mm).map_or(safe_height_mm / 25.4, |z| round_up(z / 25.4, 3));
                vec![format!("G00 Z{z:.3}")]
            }
            Raise::Manual { units: Units::Mm, safe_height_mm } => {
                let z = above(*safe_height_mm).map_or(*safe_height_mm, |z| round_up(z, 4));
                vec![format!("G00 Z{}", python_float_str(z))]
            }
            Raise::Accept { safe_height_mm } => {
                let z = above(*safe_height_mm).map_or(*safe_height_mm, |z| round_up(z, 4));
                vec![format!("G0 Z{} ", python_float_str(z))]
            }
            Raise::Restart(restart) => {
                let (state, options) = &**restart;
                sequence(state, &Options { current_z_mm: bit_mm, ..options.clone() }).lines
            }
        }
    }
}

impl Machine {
    pub(crate) fn queue_raise(&mut self, raise: Raise) {
        self.controller.queue.push_back(Queued::Raise(raise));
    }

    /// The raise's lines go to the front of the queue in its place; with the bit's height unknown it stays there
    /// until a status report tells it, and is refused after `HEIGHT_WAIT`. True while it waits.
    pub(crate) fn expand_raise(&mut self, raise: Raise, now: Duration, effects: &mut Vec<Effect>) -> bool {
        if let Some(bit_mm) = self.bit.mm() {
            self.controller.raise_deadline = None;
            let lines = raise.lines(Some(bit_mm));
            lines
                .into_iter()
                .rev()
                .for_each(|text| self.controller.queue.push_front(Queued::Line { text, boot: false }));
            return false;
        }
        let deadline = *self.controller.raise_deadline.get_or_insert(now + HEIGHT_WAIT);
        if now < deadline {
            self.controller.queue.push_front(Queued::Raise(raise));
            return true;
        }
        self.refuse_raise(&raise, effects);
        false
    }

    /// Nothing queued after the raise runs; a job waits to be started or resumed again.
    fn refuse_raise(&mut self, raise: &Raise, effects: &mut Vec<Effect>) {
        tracing::warn!("no status report told the bit's height; the move was refused");
        self.drop_user_commands();
        effects.push(broadcast(&ActivateModal::titled_alert(HEIGHT_UNKNOWN_TITLE, HEIGHT_UNKNOWN)));
        let Raise::Restart(restart) = raise else { return };
        if restart.1.change_tool {
            self.job.flow = Flow::Idle;
            self.job.paused_at = None;
            self.job.paused_units = None;
            effects.push(pause_button(PauseButton::Pause));
            self.leds.light(Led::Stop);
            return;
        }
        self.job.flow = Flow::Paused;
        self.job.paused_units = Some(self.units());
        self.job.paused_at = restart.1.z_target.map(|z| (self.index, z));
        effects.push(pause_button(PauseButton::Resume));
        self.leds.light(Led::Pause);
    }
}
