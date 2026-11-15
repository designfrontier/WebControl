//! The chain, sprocket and controller maintenance actions of the setup screens, byte for byte as
//! `Actions/actions.py` queues them, and the B10 measurements two of them wait for.

use std::time::Duration;

use serde_json::Value;
use wc_config::{python_float, python_float_str};
use wc_protocol::Action;
use wc_protocol::main::Units;

use crate::commands::round4;
use crate::machine::{Effect, Link, Machine, Queued, alert};

pub(crate) const COMMANDS: [&str; 10] = [
    "resetChainLengths",
    "testMotors",
    "wipeEEPROM",
    "setSprockets",
    "rotateSprocket",
    "setSprocketAutomatic",
    "setSprocketsZero",
    "setSprocketsDefault",
    "adjustChain",
    "setFakeServo",
];
const NOT_CONNECTED: &str = "The controller is not connected.";
/// Python slept this long after `$RST` before reading the settings back.
const AFTER_WIPE: Duration = Duration::from_secs(2);
/// adjustChain's 5 s countdown, which Python slept through one second per step.
const COUNTDOWN: Duration = Duration::from_secs(6);

/// What the next `[Measure` reply is for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Measure {
    LeftForVertical,
    RightForVertical {
        left: f64,
    },
    /// adjustChain extends the chain until it is measured, then stops it.
    Stop,
}

/// Python's `%`, whose result takes the divisor's sign.
fn python_mod(a: f64, b: f64) -> f64 {
    let rest = a % b;
    if rest != 0.0 && (rest < 0.0) != (b < 0.0) { rest + b } else { rest }
}

/// `str()` of a JSON number as Python prints an int or a float.
fn number_text(value: &Value) -> Option<String> {
    match value {
        Value::Number(n) if n.is_f64() => n.as_f64().map(python_float_str),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
    }
}

impl Machine {
    pub(crate) fn setup_action(&mut self, action: &Action, effects: &mut Vec<Effect>) -> Option<String> {
        if !matches!(self.link, Link::Connected { .. }) {
            return Some(NOT_CONNECTED.into());
        }
        let line = |text: &str| vec![text.to_string()];
        let (lines, failure) = match action.command.as_str() {
            "resetChainLengths" => (Some(self.reset_chain_lengths()), "Error with resetting chain lengths."),
            "testMotors" => (Some(line("B04 ")), "Error with testing motors"),
            "wipeEEPROM" => return self.wipe_eeprom(&action.arg),
            "setSprockets" => (self.set_sprockets(&action.arg, &action.arg1, effects), "Error with setting sprocket"),
            "rotateSprocket" => (rotate_sprocket(&action.arg, &action.arg1), "Error with setting sprocket"),
            "setSprocketAutomatic" => {
                self.measure = Some(Measure::LeftForVertical);
                (Some(line("B10 L")), "Error with setting sprockets automatically")
            }
            "setSprocketsZero" => (Some(line("B06 L0 R0 ")), "Error with setting sprockets zero value"),
            "setSprocketsDefault" => (Some(line("B08 ")), "Error with setting sprockets as default"),
            "adjustChain" => return self.adjust_chain(&action.arg),
            _ => (Some(line(if truthy(&action.arg) { "B99 ON" } else { "B99 OFF" })), "Error with changing Fake Servo"),
        };
        match lines {
            Some(lines) => {
                self.queue(lines);
                None
            }
            None => Some(failure.into()),
        }
    }

    /// B08 says the chains are at their extend length; the units line follows, as Python sends it.
    fn reset_chain_lengths(&self) -> Vec<String> {
        let units = if self.units() == Units::Inches { "G20 " } else { "G21 " };
        vec!["B08 ".into(), units.into()]
    }

    /// `$RST` gets no `ok`; the read-back waits for the controller to finish resetting.
    fn wipe_eeprom(&mut self, extent: &Value) -> Option<String> {
        let reset = match extent.as_str() {
            Some("All") => "$RST=* ",
            Some("Settings") => "$RST=$ ",
            Some("Maslow") => "$RST=# ",
            _ => return Some("Error with wiping EEPROM".into()),
        };
        self.queue([reset.to_string()]);
        self.controller.queue.push_back(Queued::Delay(AFTER_WIPE));
        self.queue(["$$".to_string()]);
        if matches!(extent.as_str(), Some("All" | "Maslow")) {
            self.queue(self.reset_chain_lengths());
        }
        None
    }

    fn sprocket_travel(&self) -> Option<f64> {
        let number = |key: &str| self.settings.value("Advanced Settings", key).and_then(python_float);
        Some(number("gearTeeth")? * number("chainPitch")?)
    }

    /// B09 turns one sprocket by `degrees` worth of chain, reversed when the chain runs under it.
    fn set_sprockets(&mut self, sprocket: &Value, degrees: &Value, effects: &mut Vec<Effect>) -> Option<Vec<String>> {
        let (sprocket, degrees) = (sprocket.as_str()?, degrees.as_f64()?);
        let mut travel = round4(self.sprocket_travel()? / 360.0 * degrees);
        let side = self.settings.value("Advanced Settings", "chainOverSprocket").and_then(Value::as_str);
        let computed = self.settings.value("Computed Settings", "chainOverSprocketComputed").and_then(Value::as_f64);
        let mismatch = match side {
            Some("Bottom") if computed != Some(2.0) => Some("set for bottom feed, but computed !=2"),
            Some("Top") if computed != Some(1.0) => Some("set for top feed, but computed != 1"),
            _ => None,
        };
        if side == Some("Bottom") {
            travel *= -1.0;
        }
        if let Some(what) = mismatch {
            let text =
                format!("mismatch between setting and computed setting. {what}. report as issue on github please");
            effects.push(alert(&text));
        }
        Some(vec!["G91 ".into(), format!("B09 {sprocket}{} ", python_float_str(travel)), "G90 ".into()])
    }

    fn adjust_chain(&mut self, chain: &Value) -> Option<String> {
        let Some(chain) = chain.as_str() else {
            return Some("Error with adjusting chain.".into());
        };
        self.controller.queue.push_back(Queued::Delay(COUNTDOWN));
        let lines = match chain {
            "left" => ["B02 L1 R0 ", "B10 L"],
            "right" => ["B02 L0 R1 ", "B10 R"],
            _ => return None,
        };
        self.measure = Some(Measure::Stop);
        self.queue(lines.map(String::from));
        None
    }

    /// messageProcessor's `measureRequest` callback for a `[Measure` reply.
    pub(crate) fn measured(&mut self, mm: f64, now: Duration, effects: &mut Vec<Effect>) {
        match self.measure.take() {
            Some(Measure::LeftForVertical) => {
                self.measure = Some(Measure::RightForVertical { left: mm });
                self.queue(["B10 R".to_string()]);
            }
            Some(Measure::RightForVertical { left }) => self.move_to_vertical(left, mm),
            Some(Measure::Stop) => {
                self.realtime("!", now, effects);
                self.drop_user_commands();
            }
            None => tracing::debug!(mm, "a measurement nobody asked for"),
        }
    }

    /// Turns each sprocket back to the nearest whole rotation, so a tooth points straight up.
    fn move_to_vertical(&mut self, left: f64, right: f64) {
        let Some(travel) = self.sprocket_travel().filter(|t| *t != 0.0) else {
            return;
        };
        let back = |length: f64| python_float_str(-python_mod(length, travel));
        self.queue(["G91 ".into(), format!("B09 L{} ", back(left)), format!("B09 R{} ", back(right)), "G90 ".into()]);
    }
}

/// B11 runs a motor at full speed for `seconds`; the sign picks the direction.
fn rotate_sprocket(sprocket: &Value, seconds: &Value) -> Option<Vec<String>> {
    let sprocket = sprocket.as_str()?;
    let time = seconds.as_f64()?;
    let line = match time > 0.0 {
        true => format!("B11 {sprocket} S100 T{}", number_text(seconds)?),
        false => {
            let magnitude = match seconds.as_i64() {
                Some(whole) => whole.unsigned_abs().to_string(),
                None => python_float_str(time.abs()),
            };
            format!("B11 {sprocket} S-100 T{magnitude}")
        }
    };
    Some(vec![line])
}
