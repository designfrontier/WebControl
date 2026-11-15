//! The PID tuning tests: the B13/B14 command `Actions.velocityPIDTest`/`positionPIDTest` build, and the data lines
//! messageProcessor collects between the firmware's start and stop markers.

use serde_json::{Map, Value};
use wc_protocol::Action;
use wc_protocol::machine::{PidKind, PidResult, UpdatePidData};
use wc_protocol::system::PidTest;

use crate::machine::{Effect, Link, Machine, broadcast};

pub(crate) const COMMANDS: [&str; 2] = ["executeVelocityPIDTest", "executePositionPIDTest"];
const NOT_CONNECTED: &str = "The controller is not connected.";
/// Python's alert for either test.
const FAILED: &str = "Error with executing velocity PID test.";

/// One test's lines so far, between its start and stop markers.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Pid {
    velocity_version: Option<Value>,
    position_version: Option<Value>,
    velocity: Option<Vec<Value>>,
    position: Option<Vec<Value>>,
}

impl Pid {
    /// Stop ends both tests, as `stopRun` clears Python's flags.
    pub(crate) fn end_tests(&mut self) {
        self.velocity = None;
        self.position = None;
    }
}

struct Spec {
    command: &'static str,
    prefix: &'static str,
    gains: [(&'static str, &'static str); 3],
    code: &'static str,
    fields: &'static [(&'static str, char)],
}

const VELOCITY: Spec = Spec {
    command: "executeVelocityPIDTest",
    prefix: "v",
    gains: [("KpV", "KpV"), ("KiV", "KiV"), ("KdV", "KdV")],
    code: "B13",
    fields: &[("Start", 'S'), ("Stop", 'F'), ("Steps", 'I'), ("Version", 'V')],
};
const POSITION: Spec = Spec {
    command: "executePositionPIDTest",
    prefix: "p",
    gains: [("KpP", "KpPos"), ("KiP", "KiPos"), ("KdP", "KdPos")],
    code: "B14",
    fields: &[("Start", 'S'), ("Stop", 'F'), ("Steps", 'I'), ("Time", 'T'), ("Version", 'V')],
};

fn spec(kind: PidKind) -> &'static Spec {
    if kind == PidKind::Velocity { &VELOCITY } else { &POSITION }
}

fn capitalised(field: &str) -> String {
    let mut chars = field.chars();
    chars.next().map(|first| first.to_ascii_uppercase().to_string() + chars.as_str()).unwrap_or_default()
}

/// The action `api/system.py`'s `pid_test_action` builds from a valid request.
pub(crate) fn test_action(test: &PidTest) -> Action {
    let spec = spec(test.kind);
    let gain = |field: &str| spec.gains.iter().find(|(name, _)| name[..2].eq_ignore_ascii_case(field)).map(|g| g.0);
    let motor = (format!("{}Motor", spec.prefix), Value::from(test.motor.clone()));
    let fields = test.fields.iter().map(|(field, text)| {
        let key = gain(field).map(String::from).unwrap_or_else(|| format!("{}{}", spec.prefix, capitalised(field)));
        (key, Value::from(text.clone()))
    });
    let arg: Map<String, Value> = std::iter::once(motor).chain(fields).collect();
    Action::new(spec.command, Value::Object(arg), Value::from(""))
}

/// Python's `float()` of a data line; `None` where it raises or JSON cannot carry the value.
fn reading(line: &str) -> Option<Value> {
    line.trim().parse::<f64>().ok().filter(|v| v.is_finite()).map(Value::from)
}

impl Machine {
    pub(crate) fn pid_action(&mut self, action: &Action) -> Option<String> {
        if !matches!(self.link, Link::Connected { .. }) {
            return Some(NOT_CONNECTED.into());
        }
        let kind = if action.command == VELOCITY.command { PidKind::Velocity } else { PidKind::Position };
        self.start_pid_test(kind, &action.arg).is_none().then(|| FAILED.into())
    }

    /// Stores the gains as typed, then queues B13/B14; `None` where Python's lookups or concatenation raise.
    fn start_pid_test(&mut self, kind: PidKind, arg: &Value) -> Option<()> {
        let spec = spec(kind);
        spec.gains.iter().try_for_each(|(name, setting)| {
            let value = arg.get(*name)?.clone();
            self.write_setting("Advanced Settings", setting, &value);
            Some(())
        })?;
        let text = |field: &str| arg.get(format!("{}{field}", spec.prefix))?.as_str().map(String::from);
        let motor = text("Motor")?;
        let words: Vec<String> = spec
            .fields
            .iter()
            .map(|(field, letter)| Some(format!("{letter}{}", text(field)?)))
            .collect::<Option<_>>()?;
        let version = arg.get(format!("{}Version", spec.prefix)).cloned();
        match kind {
            PidKind::Velocity => self.pid.velocity_version = version,
            PidKind::Position => self.pid.position_version = version,
        }
        self.queue([format!("{} {motor}1 {}", spec.code, words.join(" "))]);
        Some(())
    }

    /// messageProcessor's PID branches, in its order; true when the line was a test's and goes nowhere else.
    pub(crate) fn pid_line(&mut self, line: &str, now: std::time::Duration, effects: &mut Vec<Effect>) -> bool {
        let markers = [(PidKind::Velocity, "Velocity"), (PidKind::Position, "Position")];
        let Some((kind, event)) = markers.iter().find_map(|(kind, name)| {
            let running = self.pid_data(*kind).is_some();
            match () {
                _ if line.starts_with(&format!("--PID {name} Test Stop--")) => Some((*kind, "stop")),
                _ if running => Some((*kind, "record")),
                _ if line.starts_with(&format!("--PID {name} Test Start--")) => Some((*kind, "start")),
                _ => None,
            }
        }) else {
            return false;
        };
        match event {
            "start" => *self.pid_data(kind) = Some(Vec::new()),
            "record" => self.record_pid(kind, line),
            _ => self.finish_pid(kind, now, effects),
        }
        true
    }

    fn pid_data(&mut self, kind: PidKind) -> &mut Option<Vec<Value>> {
        match kind {
            PidKind::Velocity => &mut self.pid.velocity,
            PidKind::Position => &mut self.pid.position,
        }
    }

    fn version(&self, kind: PidKind) -> Value {
        let version = if kind == PidKind::Velocity { &self.pid.velocity_version } else { &self.pid.position_version };
        version.clone().unwrap_or_else(|| Value::from(0))
    }

    /// Version 2 keeps each line as text, version 1 its number; gain echoes and setpoint lines are dropped.
    fn record_pid(&mut self, kind: PidKind, line: &str) {
        let text_lines = self.version(kind) == "2";
        let value = match (line.contains("Kp="), text_lines) {
            (true, _) => None,
            (false, true) => (!line.contains("setpoint")).then(|| Value::from(line)),
            (false, false) => reading(line),
        };
        if let (Some(value), Some(data)) = (value, self.pid_data(kind).as_mut()) {
            data.push(value);
        }
    }

    /// A stop marker before any start has nothing to report; Python raised there.
    fn finish_pid(&mut self, kind: PidKind, now: std::time::Duration, effects: &mut Vec<Effect>) {
        let Some(data) = self.pid_data(kind).take() else {
            return;
        };
        effects.push(broadcast(&UpdatePidData::new(PidResult { result: kind, version: self.version(kind), data })));
        self.stop_run(now, effects);
    }
}
