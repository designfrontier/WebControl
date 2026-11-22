//! Parking (owner decision, 2026-11-22): the sled goes to a stored spot when a file, or the last file of a queue,
//! finishes normally, and when Park is pressed. The spot is in mm and machine coordinates: the controller keeps no
//! work offset (WebControl adds home to each file line itself), so the move goes out as is, in the current units.

use serde_json::Value;
use wc_config::{Settings, python_float, python_float_str};
use wc_protocol::main::{ParkPositionMessage, Point, Units};

use crate::clearance::Raise;
use crate::commands::round4;
use crate::machine::{Effect, Machine, alert, broadcast};
use crate::pacing::announce;

pub const SECTION: &str = "Maslow Settings";
pub const X_KEY: &str = "parkX";
pub const Y_KEY: &str = "parkY";
const MM_PER_INCH: f64 = 25.4;
pub const NOT_SET: &str = "No park position is set. Press Set Park Here with the sled where it should park, or enter \
                           Park Position X and Y in Maslow Settings.";
pub const TOOL_CHANGE: &str = "Cannot park while a tool change waits. Press Resume or STOP first.";
pub const QUEUE_BUSY: &str = "The queue is between files. Wait for it, or press Stop.";
pub const CONTROLLER_PAUSED: &str = "The controller is paused. Press Resume or STOP first.";
pub const UNPLACED: &str =
    "The sled's position is not known yet. Wait for it to stop and the controller to report it, then try again.";

/// The park position in mm, machine coordinates, when both settings hold a number.
pub fn position(settings: &Settings) -> Option<Point> {
    let read = |key: &str| settings.value(SECTION, key).and_then(python_float);
    Some(Point { x: read(X_KEY)?, y: read(Y_KEY)? })
}

/// What sent the sled to park, as the `PARK` log lines name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Trigger {
    File,
    Queue,
    Button,
}

impl Trigger {
    fn name(self) -> &'static str {
        match self {
            Trigger::File => "file",
            Trigger::Queue => "queue",
            Trigger::Button => "button",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Park {
    /// Since the run started something ended it abnormally or the controller refused a line, so it never parks.
    spoiled: bool,
    /// The run ended normally; it parks once the controller has finished every line.
    pending: Option<Trigger>,
    shown: Option<Point>,
}

impl Park {
    pub fn new(settings: &Settings) -> Park {
        Park { spoiled: false, pending: None, shown: position(settings) }
    }
}

impl Machine {
    pub(crate) fn park_run_started(&mut self) {
        self.park.spoiled = false;
        self.park.pending = None;
    }

    pub(crate) fn park_spoiled(&mut self) {
        self.park.spoiled = true;
        self.park.pending = None;
    }

    /// The user sent the machine something else before the park went out.
    pub(crate) fn park_cancelled(&mut self) {
        self.park.pending = None;
    }

    pub(crate) fn park_waits(&self) -> bool {
        self.park.pending.is_some()
    }

    pub(crate) fn park_when_finished(&mut self, trigger: Trigger) {
        if !self.park.spoiled && position(&self.settings).is_some() {
            self.park.pending = Some(trigger);
        }
    }

    /// Before each pump: a finished run parks once the controller is idle, through the same gates as Park.
    pub(crate) fn park_tick(&mut self, effects: &mut Vec<Effect>) {
        if !self.controller_idle() || self.queue_busy() {
            return;
        }
        let Some(trigger) = self.park.pending.take() else { return };
        if let Some(why) = self.park_refusal().or_else(|| self.queue_park(trigger, effects).err()) {
            announce(format!("PARK SKIPPED trigger={} reason={why}", trigger.name()), effects);
            effects.push(alert(&format!("The job finished, but the sled did not park. {why}")));
        }
    }

    /// What holds the controller beyond the action gates: a scan, an upgrade, a missed restart.
    fn park_refusal(&self) -> Option<String> {
        let refusal = match () {
            _ if self.optical_holds() => Some(crate::optical::RUNNING),
            _ if self.upgrade.holds_link() => Some("Wait for the firmware upgrade to finish."),
            _ if self.alarm_held() => Some(crate::alarm::HELD),
            _ => self.restart_refusal(),
        };
        refusal.map(String::from)
    }

    /// The Park button, once `action` has passed every gate an idle-only move has.
    pub(crate) fn park_action(&mut self, effects: &mut Vec<Effect>) -> Option<String> {
        let refusal = match () {
            _ if self.job.manual_z.is_some() => Some(TOOL_CHANGE),
            _ if self.queue_busy() => Some(QUEUE_BUSY),
            _ if self.job.firmware_paused => Some(CONTROLLER_PAUSED),
            _ => None,
        };
        refusal.map(String::from).or_else(|| self.queue_park(Trigger::Button, effects).err())
    }

    fn queue_park(&mut self, trigger: Trigger, effects: &mut Vec<Effect>) -> Result<(), String> {
        let park = position(&self.settings).ok_or(NOT_SET)?;
        self.on_bed(park)?;
        let safe_height_mm = self.safe_height("parking")?;
        let units = self.units();
        let scale = if units == Units::Inches { 1.0 / MM_PER_INCH } else { 1.0 };
        let [x, y] = [park.x, park.y].map(|mm| python_float_str(round4(mm * scale)));
        let (mm_x, mm_y) = (python_float_str(park.x), python_float_str(park.y));
        let name = if units == Units::Inches { "INCHES" } else { "MM" };
        let line = format!("PARK START trigger={} x_mm={mm_x} y_mm={mm_y} units={name} x={x} y={y}", trigger.name());
        announce(line, effects);
        // The units go out again so the move is read in the units it was converted to.
        let units_line = if units == Units::Inches { "G20 " } else { "G21 " };
        self.queue([units_line.to_string(), "G90 ".into()]);
        self.queue_raise(Raise::Manual { units, safe_height_mm });
        self.queue([format!("G00 X{x} Y{y} ")]);
        Ok(())
    }

    fn on_bed(&self, park: Point) -> Result<(), String> {
        let bed = |key: &str| self.settings.number("Maslow Settings", key);
        let (width, height) = (bed("bedWidth"), bed("bedHeight"));
        if park.x.abs() <= width / 2.0 && park.y.abs() <= height / 2.0 {
            return Ok(());
        }
        let (x, y) = (python_float_str(park.x), python_float_str(park.y));
        Err(format!(
            "The park position ({x}, {y} mm) is off the {width} x {height} mm bed. Change Park Position X and Y in \
             Maslow Settings."
        ))
    }

    /// Set Park Here: the sled's last reported position, in mm.
    pub(crate) fn define_park(&mut self, effects: &mut Vec<Effect>) -> Option<String> {
        let settled = self.controller.queue.is_empty() && self.controller.accounting.is_empty();
        let Some([x, y]) = self.sled.reported_mm().filter(|_| settled) else {
            return Some(UNPLACED.into());
        };
        let [x, y] = [x, y].map(|mm| python_float_str(round4(mm)));
        self.set_setting(SECTION, X_KEY, Value::from(x.clone()));
        self.set_setting(SECTION, Y_KEY, Value::from(y.clone()));
        announce(format!("PARK SET x_mm={x} y_mm={y}"), effects);
        None
    }

    /// After each event: every client hears a new park position.
    pub(crate) fn publish_park(&mut self, effects: &mut Vec<Effect>) {
        if std::sync::Arc::ptr_eq(&self.settings, &self.saved) {
            return;
        }
        let position = position(&self.settings);
        if position != self.park.shown {
            self.park.shown = position;
            effects.push(broadcast(&ParkPositionMessage { position }));
        }
    }
}
