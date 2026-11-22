//! Parking (owner decision, 2026-11-22): the sled goes to a stored spot when a file, or the last file of a queue,
//! finishes normally, and when Park is pressed. The spot is in mm and machine coordinates: the controller keeps no
//! work offset (WebControl adds home to each file line itself), so the move goes out as is, in the current units.

use std::time::Duration;

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
pub const TOOL_CHANGE_KEY: &str = "parkForToolChange";
const MM_PER_INCH: f64 = 25.4;
pub const NOT_SET: &str = "No park position is set. Press Set Park Here with the sled where it should park, or enter \
                           Park Position X and Y in Maslow Settings.";
pub const TOOL_CHANGE: &str = "Cannot park while a tool change waits. Press Resume or STOP first.";
pub const QUEUE_BUSY: &str = "The queue is between files. Wait for it, or press Stop.";
pub const CONTROLLER_PAUSED: &str = "The controller is paused. Press Resume or STOP first.";
pub const RETURNED: &str = "The sled went back to where the job was, at the safe height. Lower the bit close to the work \
                            and press Touch Z again.";
const XY_UNKNOWN: &str = "Where the sled is is not known, so it could not come back.";
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
    ToolChange,
}

impl Trigger {
    fn name(self) -> &'static str {
        match self {
            Trigger::File => "file",
            Trigger::Queue => "queue",
            Trigger::Button => "button",
            Trigger::ToolChange => "tool_change",
        }
    }
}

struct ParkLines {
    log: String,
    safe_height_mm: f64,
    xy: [f64; 2],
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Park {
    /// The file's last line went out; it parks once the controller has finished every line.
    pending: Option<Trigger>,
    /// A tool change prompted; it parks once the firmware has paused for it.
    tool_change_asked: bool,
    /// Parked for a tool change between queued files: where the sled was, in mm, to go back to.
    return_to: Option<[f64; 2]>,
    shown: Option<Point>,
}

impl Park {
    pub fn new(settings: &Settings) -> Park {
        Park { pending: None, tool_change_asked: false, return_to: None, shown: position(settings) }
    }
}

impl Machine {
    /// The user sent the machine something else, or something ended the job, before the park went out.
    pub(crate) fn park_cancelled(&mut self) {
        self.park.pending = None;
    }

    /// The tool change is over: resumed, or ended by a stop.
    pub(crate) fn park_tool_change_ended(&mut self) {
        self.park.tool_change_asked = false;
        self.park.return_to = None;
    }

    /// A job or the queue asked for a tool; with Park for Tool Changes on, it parks once the firmware has paused.
    pub(crate) fn park_tool_change_asked(&mut self) {
        let waits = self.job.flow.in_job() || self.queue_changing_tool();
        self.park.tool_change_asked = waits && self.settings.number(SECTION, TOOL_CHANGE_KEY) != 0.0;
    }

    /// The tool change prompt is up and the firmware paused (or never pauses, for a tool it already has).
    pub(crate) fn park_for_tool_change(&mut self, now: Duration, effects: &mut Vec<Effect>) {
        if !std::mem::take(&mut self.park.tool_change_asked) {
            return;
        }
        let refused = self.park_refusal().or_else(|| self.tool_change_return_refusal());
        let parked = refused.map_or_else(
            || {
                let from = self.sled.xy_mm();
                let firmware_paused = self.job.firmware_paused;
                self.queue_park_checked(Trigger::ToolChange).map(|lines| {
                    if firmware_paused {
                        // Streaming stays held; the firmware runs the park now, as it runs a jog.
                        self.release_firmware(now, effects);
                    }
                    self.park.return_to = from.filter(|_| !self.job.flow.in_job());
                    self.queue_park_lines(lines, effects);
                })
            },
            Err,
        );
        if let Err(why) = parked {
            announce(format!("PARK SKIPPED trigger={} reason={why}", Trigger::ToolChange.name()), effects);
            effects.push(alert(&format!("The sled did not park for the tool change. {why}")));
        }
    }

    /// Resume must be able to bring the sled back: a job returns to its line, the queue to where the sled was.
    fn tool_change_return_refusal(&self) -> Option<String> {
        let back = if self.job.flow.in_job() { self.can_return_to_line() } else { self.sled.xy_mm().is_some() };
        (!back).then(|| XY_UNKNOWN.to_string())
    }

    /// Before the next queued file starts: back to where the sled was when it parked for the tool change.
    /// True when it queued the return, which raises Z first.
    pub(crate) fn queue_return_from_park(&mut self, safe_height_mm: f64) -> bool {
        let Some(xy) = self.park.return_to.take() else { return false };
        self.queue_raised_travel(safe_height_mm, xy);
        true
    }

    /// Touch Z while parked between queued files goes back to the work first, and probes only when pressed again.
    pub(crate) fn touch_z_from_park(&mut self) -> Option<String> {
        let xy = self.park.return_to?;
        let safe_height_mm = match self.safe_height("returning to the work") {
            Ok(height) => height,
            Err(refused) => return Some(refused),
        };
        self.park.return_to = None;
        self.queue_raised_travel(safe_height_mm, xy);
        Some(RETURNED.into())
    }

    pub(crate) fn park_when_finished(&mut self, trigger: Trigger) {
        if position(&self.settings).is_some() {
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
        let lines = self.queue_park_checked(trigger)?;
        self.queue_park_lines(lines, effects);
        Ok(())
    }

    /// The park's log line and safe height, once every check has passed; nothing is queued yet.
    fn queue_park_checked(&self, trigger: Trigger) -> Result<ParkLines, String> {
        let park = position(&self.settings).ok_or(NOT_SET)?;
        self.on_bed(park)?;
        let safe_height_mm = self.safe_height("parking")?;
        let units = self.units();
        let [x, y] = self.in_units([park.x, park.y]);
        let (mm_x, mm_y) = (python_float_str(park.x), python_float_str(park.y));
        let name = if units == Units::Inches { "INCHES" } else { "MM" };
        let log = format!("PARK START trigger={} x_mm={mm_x} y_mm={mm_y} units={name} x={x} y={y}", trigger.name());
        Ok(ParkLines { log, safe_height_mm, xy: [park.x, park.y] })
    }

    fn queue_park_lines(&mut self, lines: ParkLines, effects: &mut Vec<Effect>) {
        announce(lines.log, effects);
        self.queue_raised_travel(lines.safe_height_mm, lines.xy);
    }

    /// The units the move was converted to and G90, Z up as Home raises it, then a rapid to machine X and Y in mm.
    fn queue_raised_travel(&mut self, safe_height_mm: f64, xy_mm: [f64; 2]) {
        let units = self.units();
        let units_line = if units == Units::Inches { "G20 " } else { "G21 " };
        self.queue([units_line.to_string(), "G90 ".into()]);
        self.queue_raise(Raise::Manual { units, safe_height_mm });
        let [x, y] = self.in_units(xy_mm);
        self.queue([format!("G00 X{x} Y{y} ")]);
    }

    /// X and Y in mm, written in the units WebControl is in.
    fn in_units(&self, xy_mm: [f64; 2]) -> [String; 2] {
        let scale = if self.units() == Units::Inches { 1.0 / MM_PER_INCH } else { 1.0 };
        xy_mm.map(|mm| python_float_str(round4(mm * scale)))
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
