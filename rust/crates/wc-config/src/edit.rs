//! `Config.setValue`'s conversions and `Config.computeSettings`, without their controller sync, which
//! `wc-firmware` adds from the `Stored` each returns.

use serde_json::{Map, Value};

use crate::{Settings, python_float_str};

/// One value written: what it replaced and the controller key it maps to, if any.
#[derive(Debug, Clone, PartialEq)]
pub struct Stored {
    pub section: String,
    pub key: String,
    pub previous: Value,
    pub firmware_key: Option<u32>,
}

/// The error array goes out as `$O` lines, never through `setValue`'s int branch.
const ERROR_ARRAY_KEY: u32 = 85;
const ADVANCED: &str = "Advanced Settings";
const COMPUTED: &str = "Computed Settings";
const POSITION_GAINS: [&str; 4] = ["KpPos", "KiPos", "KdPos", "propWeight"];
const POSITION_GAINS_Z: [&str; 4] = ["KpPosZ", "KiPosZ", "KdPosZ", "propWeightZ"];
const VELOCITY_GAINS: [&str; 3] = ["KpV", "KiV", "KdV"];
const VELOCITY_GAINS_Z: [&str; 3] = ["KpVZ", "KiVZ", "KdVZ"];

/// Python's `float()` of a stored or posted value; `None` where it raises or gives a value JSON cannot hold.
pub fn python_float(value: &Value) -> Option<f64> {
    let number = match value {
        Value::Number(n) => n.as_f64(),
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    };
    number.filter(|n| n.is_finite())
}

/// Python's `int()`: text must be a whole number, floats truncate.
fn python_int(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().filter(|f| f.is_finite()).map(|f| f.trunc() as i64)),
        Value::Bool(b) => Some(i64::from(*b)),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

/// Python's `==` between two loaded values: numbers and booleans compare by value.
pub fn python_eq(a: &Value, b: &Value) -> bool {
    let numeric = |v: &Value| match v {
        Value::Number(n) => n.as_f64(),
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        _ => None,
    };
    match (numeric(a), numeric(b)) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

/// The bool branch: `True`/`"on"`/`"1"`/`1` store 1, anything else 0.
fn bool_value(value: &Value) -> Value {
    let on = match value {
        Value::Bool(b) => *b,
        Value::String(text) => text == "on" || text == "1",
        Value::Number(n) => n.as_f64() == Some(1.0),
        _ => false,
    };
    Value::from(i64::from(on))
}

impl Settings {
    fn option_mut(&mut self, section: &str, key: &str) -> Option<&mut Map<String, Value>> {
        let wanted = key.to_lowercase();
        self.sections
            .get_mut(section)?
            .as_array_mut()?
            .iter_mut()
            .filter_map(Value::as_object_mut)
            .find(|option| option.get("key").and_then(Value::as_str).is_some_and(|k| k.to_lowercase() == wanted))
    }

    /// `Config.setValue`: converted by the option's `type`; `None` if it does not exist or Python's conversion
    /// raises, which leaves the stored value alone.
    pub fn store(&mut self, section: &str, key: &str, value: &Value) -> Option<Stored> {
        let option = self.option_mut(section, key)?;
        let kind = option.get("type").and_then(Value::as_str).unwrap_or("string");
        let new = match kind {
            "float" => Value::from(python_float(value)?),
            "int" => Value::from(python_int(value)?),
            "bool" => bool_value(value),
            _ => value.clone(),
        };
        let firmware_key = option
            .get("firmwareKey")
            .and_then(Value::as_u64)
            .and_then(|k| u32::try_from(k).ok())
            .filter(|k| kind != "int" || *k != ERROR_ARRAY_KEY);
        let previous = option.insert("value".into(), new).unwrap_or(Value::Null);
        let key = option.get("key").and_then(Value::as_str).unwrap_or(key).to_string();
        Some(Stored { section: section.into(), key, previous, firmware_key })
    }

    fn store_if_changed(&mut self, key: &str, value: Value, stored: &mut Vec<Stored>) {
        if !self.value(COMPUTED, key).is_some_and(|current| python_eq(current, &value)) {
            stored.extend(self.store(COMPUTED, key, &value));
        }
    }

    /// The advanced value as a float when `enabled`, else its default.
    fn gain(&self, key: &str, enabled: bool) -> Option<Value> {
        match enabled {
            true => self.value(ADVANCED, key).and_then(python_float).map(Value::from),
            false => self.option(ADVANCED, key)?.get("default").cloned(),
        }
    }

    /// `Config.computeSettings(doAll=True)`: the derived settings, each stored where it changed, in Python's order.
    pub fn compute(&mut self) -> Vec<Stored> {
        let mut stored = Vec::new();
        let quadrilateral = self.value(ADVANCED, "kinematicsType") == Some(&Value::from("Quadrilateral"));
        self.store_if_changed(
            "kinematicsTypeComputed",
            Value::from(if quadrilateral { "1" } else { "2" }),
            &mut stored,
        );
        let number = |key: &str| self.value(ADVANCED, key).and_then(python_float);
        if let Some(per_rotation) = number("gearTeeth").zip(number("chainPitch")).map(|(t, p)| t * p) {
            self.store_if_changed("distPerRot", Value::from(python_float_str(per_rotation)), &mut stored);
        }
        // Python reads enablePosPIDValues for the velocity gains too.
        let enabled = self.value(ADVANCED, "enablePosPIDValues").and_then(python_int) == Some(1);
        let mains = POSITION_GAINS.iter().map(|k| (*k, format!("{k}Main")));
        let mains = mains.chain(POSITION_GAINS_Z.iter().map(|k| (*k, k.to_string())));
        let mains = mains.chain(VELOCITY_GAINS.iter().map(|k| (*k, format!("{k}Main"))));
        let mains: Vec<(&str, String)> = mains.chain(VELOCITY_GAINS_Z.iter().map(|k| (*k, k.to_string()))).collect();
        mains.iter().for_each(|(key, main)| {
            if let Some(gain) = self.gain(key, enabled) {
                self.store_if_changed(main, gain, &mut stored);
            }
        });
        let chain = match self.value(ADVANCED, "chainOverSprocket").and_then(Value::as_str) {
            Some("Top") => Some(1),
            Some("Bottom") => Some(2),
            _ => None,
        };
        if let Some(code) = chain {
            self.store_if_changed("chainOverSprocketComputed", Value::from(code), &mut stored);
        }
        let pwm = match self.value(ADVANCED, "fPWM").and_then(Value::as_str) {
            Some("31,000Hz") => 1,
            Some("4,100Hz") => 2,
            _ => 3,
        };
        self.store_if_changed("fPWMComputed", Value::from(pwm), &mut stored);
        stored
    }
}
