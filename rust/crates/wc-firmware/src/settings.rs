//! `$n=value` lines (B9): how a stored setting is written, how a `$$` reply compares with it, and the full push the
//! Rust backend sends after every boot (contract/serial wart 4).

use serde_json::Value;
use wc_config::{FirmwareOption, Settings, python_str};

/// The firmware stores these as (unsigned) ints, so it truncates any fraction written to them.
pub const INTEGER_KEYS: [u32; 4] = [9, 10, 11, 15];
/// The error array goes out as `$O` lines, and the firmware rejects `$86` and `$144`.
pub const UNSENDABLE_KEYS: [u32; 3] = [85, 86, 144];
/// Settings.cpp sets the sprocket radius R from `$13` on a write only; it boots with R = 10.1.
pub const WRITE_ONLY_KEY: u32 = 13;
/// Arduino's `Serial.print(float, 8)` resolution, which is all a `$$` reply carries.
const PRINTED_RESOLUTION: f64 = 1e-8;
const CUSTOM_FIRMWARE: f64 = 100.0;

fn is_optical(key: u32) -> bool {
    (87..=98).contains(&key)
}

/// `^\$(\d+)=\s*(\S+)`; Arduino prints `nan`, `inf` or `ovf` for values it can't show, all read as NaN.
pub fn parse_setting_reply(line: &str) -> Option<(u32, f64)> {
    let (key, rest) = line.strip_prefix('$')?.split_once('=')?;
    let key = key.parse().ok().filter(|_| key.chars().all(|c| c.is_ascii_digit()))?;
    let text = rest.trim_start().split_whitespace().next()?;
    let value = text.parse::<f64>().ok().filter(|v| v.is_finite()).unwrap_or(f64::NAN);
    Some((key, value))
}

/// `Config.firmwareKeyValue`: text goes out as stored; numbers with about six significant decimals.
pub fn firmware_key_value(value: &Value) -> String {
    let number = match value {
        Value::Number(n) => n.as_f64().unwrap_or_default(),
        Value::Bool(b) => f64::from(u8::from(*b)),
        other => return python_str(other),
    };
    // Python's math.log(x, 10) is log(x) / log(10), which is what makes 1000 land on 2.9999999999999996.
    let digits = if number == 0.0 { 0 } else { (number.abs().ln() / 10f64.ln()).ceil() as i64 };
    let precision = 6 - digits;
    if precision < 0 {
        return python_str(value);
    }
    let width = (7 - digits).max(7).max(digits.abs()) as usize;
    format!("{number:>width$.prec$}", prec = precision as usize)
}

pub fn setting_line(key: u32, value: &Value) -> String {
    format!("${key}={}", firmware_key_value(value))
}

/// The number Python's `float()` reads from a stored value, if it reads one.
fn stored_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        Value::String(s) => wc_config::parse_number(s),
        _ => None,
    }
}

fn is_close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * a.abs().max(b.abs())
}

/// Whether a `$$` reply agrees with the stored value at the precision the firmware prints.
pub fn values_match(key: u32, stored: &Value, reported: f64) -> bool {
    if stored == &Value::String(String::new()) {
        return true;
    }
    let Some(stored) = stored_number(stored) else {
        return false;
    };
    if reported.is_nan() {
        return false;
    }
    if INTEGER_KEYS.contains(&key) {
        return stored.trunc() == reported.trunc();
    }
    is_close(stored, reported) || (stored - reported).abs() <= PRINTED_RESOLUTION
}

/// What WebControl stores for a value adopted from `$$`: eight decimals, trailing zeros dropped.
pub fn adopted_text(value: f64) -> String {
    let text = format!("{value:.8}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// `spindleAutomate` is stored by name and written as its code.
fn spindle_code(value: &Value) -> Value {
    let code = match value.as_str() {
        Some("Servo") => 1,
        Some("Relay_High") => 2,
        Some("Relay_Low") => 3,
        _ => 0,
    };
    Value::from(code)
}

fn sendable(option: &FirmwareOption, firmware: f64) -> bool {
    let key = option.firmware_key;
    !UNSENDABLE_KEYS.contains(&key) && (firmware >= CUSTOM_FIRMWARE || (!is_optical(key) && !option.custom))
}

/// The value as written; `None` for text that is not a number, which never goes out.
fn written_value(option: &FirmwareOption, value: &Value) -> Option<Value> {
    if option.key == "spindleAutomate" {
        return Some(spindle_code(value));
    }
    let number = stored_number(value)?;
    match (option.firmware_key, value) {
        (WRITE_ONLY_KEY, _) => Some(Value::from(number)),
        _ => Some(value.clone()),
    }
}

/// What a boot push sends, and the defaults it stores for keys that were unknown after an EEPROM reset.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Push {
    pub lines: Vec<String>,
    pub adopted: Vec<(String, String, Value)>,
}

/// Every setting the firmware takes, in key order; an unknown (blank) one is left to the controller unless
/// its EEPROM was reset, when it gets its default.
pub fn boot_push(settings: &Settings, firmware: f64, eeprom_reset: bool) -> Push {
    let mut options: Vec<FirmwareOption> = settings.firmware_options().filter(|o| sendable(o, firmware)).collect();
    options.sort_by_key(|o| o.firmware_key);
    options.dedup_by_key(|o| o.firmware_key);
    options.into_iter().fold(Push::default(), |mut push, option| {
        let blank = option.value == Value::String(String::new());
        let value = match (blank, eeprom_reset) {
            (false, _) => option.value.clone(),
            (true, true) => {
                let default = Value::String(python_str(&option.default));
                push.adopted.push((option.section.clone(), option.key.clone(), default.clone()));
                default
            }
            (true, false) => return push,
        };
        if let Some(written) = written_value(&option, &value) {
            push.lines.push(setting_line(option.firmware_key, &written));
        }
        push
    })
}

/// What to do about one `$n=value` line of a `$$` reply.
#[derive(Debug, Clone, PartialEq)]
pub enum Sync {
    /// WebControl never knew the value: store the controller's.
    Adopt {
        section: String,
        key: String,
        value: Value,
    },
    /// It differs from the stored value: write the stored one again.
    Rewrite(String),
    Agrees,
}

pub fn sync_reported(settings: &Settings, firmware: f64, key: u32, reported: f64) -> Sync {
    let Some(option) = settings.firmware_options().find(|o| o.firmware_key == key && sendable(o, firmware)) else {
        return Sync::Agrees;
    };
    if option.value == Value::String(String::new()) {
        return match reported.is_nan() {
            true => Sync::Agrees,
            false => Sync::Adopt { section: option.section, key: option.key, value: adopted_text(reported).into() },
        };
    }
    let stored = if option.key == "spindleAutomate" { spindle_code(&option.value) } else { option.value.clone() };
    match (values_match(key, &stored, reported), written_value(&option, &option.value)) {
        (false, Some(written)) => Sync::Rewrite(setting_line(key, &written)),
        _ => Sync::Agrees,
    }
}
