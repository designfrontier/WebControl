//! Owner decision (2026-11-15): before any XY travel that starts a resume, a restart or a manual return, Z goes to the
//! higher of the safe height and the bit's own height, so nothing before the travel lowers the bit.

use wc_config::python_float_str;
use wc_gcode::commanded_z::CommandedZ;
use wc_gcode::restart::{GcodeState, Options, round_up, sequence};
use wc_protocol::main::Units;

use crate::machine::{Machine, Queued};

/// Where the bit is: the Z WebControl commanded, or else the last status report's, one display step higher.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BitHeight {
    commanded: CommandedZ,
    /// MPos z read since the last line that moved Z, in mm.
    reported_mm: Option<f64>,
}

impl BitHeight {
    pub fn new(units: Units) -> BitHeight {
        BitHeight { commanded: CommandedZ::new(units), reported_mm: None }
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
        }
    }

    /// MPos has two decimals, so the bit may be up to one step above it.
    pub fn reported(&mut self, z: f64) {
        let mm_per_unit = if self.commanded.units() == Units::Inches { 25.4 } else { 1.0 };
        self.reported_mm = z.is_finite().then_some((z + 0.01) * mm_per_unit);
    }

    /// A Stop, an alarm, a probe or a reboot left the bit somewhere a status report must tell.
    pub fn lost(&mut self) {
        self.commanded.set_z_mm(None);
        self.reported_mm = None;
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

    /// The raise's lines go to the front of the queue, to be written in its place.
    pub(crate) fn expand_raise(&mut self, raise: &Raise) {
        let lines = raise.lines(self.bit.mm());
        lines.into_iter().rev().for_each(|text| self.controller.queue.push_front(Queued::Line { text, boot: false }));
    }
}
