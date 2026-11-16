//! `$n=value` lines (B9): how a stored setting is written, how a `$$` reply compares with it, what a boot writes
//! after reading `$$` (B2), and the full push after an EEPROM reset.

use serde_json::Value;
use wc_config::{FirmwareOption, Settings, Stored, python_str};

use crate::number::{firmware_range, firmware_text, in_firmware_range};

/// The firmware stores these as (unsigned) ints, so it truncates any fraction written to them.
pub const INTEGER_KEYS: [u32; 4] = [9, 10, 11, 15];
/// The error array goes out as `$O` lines, and the firmware rejects `$86` and `$144`.
pub const UNSENDABLE_KEYS: [u32; 3] = [85, 86, 144];
/// Settings.cpp sets the sprocket radius R from `$13` on a write only; it boots with R = 10.1.
pub const WRITE_ONLY_KEY: u32 = 13;
/// A write of these re-runs `setupAxes()` on the live board (`spec/firmware.md` B2), after which the motors no
/// longer follow; the controller must reboot before it moves again.
pub const REINIT_KEYS: [u32; 1] = [38];
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
    let text = rest.split_whitespace().next()?;
    let value = text.parse::<f64>().ok().filter(|v| v.is_finite()).unwrap_or(f64::NAN);
    Some((key, value))
}

/// `Config.firmwareKeyValue`: text as stored; numbers with about six significant decimals. Only the fallback for
/// a value no text makes the firmware read exactly.
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

/// The `$key=` line that makes the firmware store the float32 nearest `value` (`number::firmware_text`).
pub fn setting_line(key: u32, value: &Value) -> String {
    let fallback = firmware_key_value(value);
    let text = stored_number(value).map_or_else(|| fallback.clone(), |number| firmware_text(key, number, &fallback));
    format!("${key}={text}")
}

/// The key a `$n=value` line writes.
pub fn setting_key(line: &str) -> Option<u32> {
    line.trim().strip_prefix('$')?.split_once('=')?.0.trim().parse().ok()
}

pub fn reinitialises_axes(line: &str) -> bool {
    setting_key(line).is_some_and(|key| REINIT_KEYS.contains(&key))
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
    /// WebControl never knew the value: store the controller's, brought into the firmware's range, and write that
    /// back when the controller's was outside it.
    Adopt {
        section: String,
        key: String,
        value: Value,
        repair: Option<String>,
    },
    /// It differs from the stored value, or the firmware refuses to use it: write the stored one again.
    Rewrite(String),
    Agrees,
}

fn adopt(option: FirmwareOption, reported: f64) -> Sync {
    let (low, high) = firmware_range(option.firmware_key).map_or((f64::MIN, f64::MAX), |(l, h)| (l.into(), h.into()));
    let value = Value::from(adopted_text(reported.clamp(low, high)));
    let repair = (!in_firmware_range(option.firmware_key, reported)).then(|| setting_line(option.firmware_key, &value));
    Sync::Adopt { section: option.section, key: option.key, value, repair }
}

/// The stored value as the firmware gets it: brought into the key's range, as `setting_line` writes it.
fn in_range(key: u32, stored: Value) -> Value {
    let range = firmware_range(key).map(|(low, high)| (f64::from(low), f64::from(high)));
    match (range, stored_number(&stored)) {
        (Some((low, high)), Some(number)) if !(low..=high).contains(&number) => Value::from(number.clamp(low, high)),
        _ => stored,
    }
}

pub fn sync_reported(settings: &Settings, firmware: f64, key: u32, reported: f64) -> Sync {
    let Some(option) = settings.firmware_options().find(|o| o.firmware_key == key && sendable(o, firmware)) else {
        return Sync::Agrees;
    };
    if option.value == Value::String(String::new()) {
        return if reported.is_nan() { Sync::Agrees } else { adopt(option, reported) };
    }
    let stored = if option.key == "spindleAutomate" { spindle_code(&option.value) } else { option.value.clone() };
    let held = values_match(key, &in_range(key, stored), reported) && in_firmware_range(key, reported);
    match (held, written_value(&option, &option.value)) {
        (false, Some(written)) => Sync::Rewrite(setting_line(key, &written)),
        _ => Sync::Agrees,
    }
}

/// The `$13` line a boot always writes, since only that write sets R.
fn radius_line(settings: &Settings, firmware: f64) -> Option<String> {
    let option = settings.firmware_options().find(|o| o.firmware_key == WRITE_ONLY_KEY && sendable(o, firmware))?;
    written_value(&option, &option.value).map(|value| setting_line(WRITE_ONLY_KEY, &value))
}

/// B2: the boot's `$$` against the stored settings. Unknown keys are adopted, only keys that differ are written, and
/// `$13` goes once. Keys `$$` did not report are left alone, as Python left them.
pub fn boot_sync(settings: &Settings, firmware: f64, reported: &[(u32, f64)]) -> Push {
    let mut push = reported.iter().fold(Push::default(), |mut push, &(key, value)| {
        match sync_reported(settings, firmware, key, value) {
            Sync::Adopt { section, key, value, repair } => {
                push.adopted.push((section, key, value));
                push.lines.extend(repair);
            }
            Sync::Rewrite(line) => push.lines.push(line),
            Sync::Agrees => {}
        }
        push
    });
    let radius = radius_line(settings, firmware).filter(|_| !push.lines.iter().any(|l| setting_key(l) == Some(13)));
    push.lines.extend(radius);
    push.lines.sort_by_key(|line| setting_key(line));
    push.lines.dedup_by_key(|line| setting_key(line));
    push
}

/// A sync write that changes what `$$` showed the controller holding.
#[derive(Debug, Clone, PartialEq)]
pub struct Correction {
    pub key: u32,
    /// The WebControl setting the key belongs to.
    pub setting: String,
    pub controller: f64,
    pub written: String,
    /// The controller's value was one the firmware refuses to use (a PID weight or gain out of range).
    pub repair: bool,
}

/// The `lines` that change a value in `reported`; rewriting a value the controller already holds, as the `$13`
/// every boot sends does, is none.
pub fn corrections(settings: &Settings, firmware: f64, lines: &[String], reported: &[(u32, f64)]) -> Vec<Correction> {
    lines
        .iter()
        .filter_map(|line| {
            let key = setting_key(line)?;
            let &(_, controller) = reported.iter().rev().find(|(k, _)| *k == key)?;
            let written = line.split_once('=')?.1.trim().to_string();
            let repair = !in_firmware_range(key, controller);
            let held = written.parse::<f64>().is_ok_and(|w| values_match(key, &Value::from(w), controller));
            let setting = settings.firmware_options().find(|o| o.firmware_key == key && sendable(o, firmware));
            (repair || !held).then(|| Correction {
                key,
                setting: setting.map(|o| o.key).unwrap_or_default(),
                controller,
                written,
                repair,
            })
        })
        .collect()
}

/// Drops each write of a re-initialising key whose value `$$` showed the controller already holds.
pub fn skip_held_reinit_keys(push: &mut Push, reported: &[(u32, f64)]) {
    let held = |line: &String| {
        let key = setting_key(line).filter(|key| REINIT_KEYS.contains(key));
        let found = key.and_then(|key| reported.iter().find(|(k, _)| *k == key));
        let written = line.split_once('=').and_then(|(_, value)| value.trim().parse::<f64>().ok());
        matches!((found, written), (Some(&(key, value)), Some(w)) if values_match(key, &Value::from(w), value))
    };
    push.lines.retain(|line| !held(line));
}

/// Python's `isPercentClose` with its default 0.01% tolerance.
fn percent_close(a: f64, b: f64) -> bool {
    if b == 0.0 { is_close(a, b) } else { ((a / b).abs() - 1.0).abs() < 1e-4 }
}

/// The non-reported branch of `firmwareValuesMatch`; text Python could not compare is never sent.
fn unchanged(stored: &Value, previous: &Value) -> bool {
    let blank = |v: &Value| v == &Value::String(String::new());
    match (blank(stored), blank(previous), stored_number(stored), stored_number(previous)) {
        (true, _, _, _) => true,
        (_, true, _, _) => false,
        (_, _, Some(a), Some(b)) => is_close(a, b),
        _ => true,
    }
}

/// `syncFirmwareKey` after `setValue`: the `$` line for a stored value the controller should now get.
pub fn written_line(settings: &Settings, firmware: f64, stored: &Stored) -> Option<String> {
    let key = stored.firmware_key?;
    let option = settings.firmware_options().find(|o| o.section == stored.section && o.key == stored.key)?;
    let applies = firmware > CUSTOM_FIRMWARE || !option.custom;
    let spindle = option.key == "spindleAutomate";
    let code = |value: &Value| if spindle { spindle_code(value) } else { value.clone() };
    let (value, previous) = (code(&option.value), code(&stored.previous));
    let changed = match key {
        k if UNSENDABLE_KEYS.contains(&k) => false,
        k if is_optical(k) => {
            firmware >= CUSTOM_FIRMWARE
                && stored_number(&value).zip(stored_number(&previous)).is_some_and(|(a, b)| !percent_close(a, b))
        }
        _ => !unchanged(&value, &previous),
    };
    (applies && changed && stored_number(&value).is_some()).then(|| setting_line(key, &value))
}

/// `Config.setValue` then `computeSettings`: every `$` line the write sends, in Python's order; `None` if nothing
/// was stored.
pub fn write(settings: &mut Settings, firmware: f64, section: &str, key: &str, value: &Value) -> Option<Vec<String>> {
    let stored = settings.store(section, key, value)?;
    let line = written_line(settings, firmware, &stored);
    Some(line.into_iter().chain(recompute(settings, firmware)).collect())
}

/// `Config.computeSettings(doAll=True)` on its own, as startup, the imports and a restore run it: the `$` lines
/// for the derived settings it changed.
pub fn recompute(settings: &mut Settings, firmware: f64) -> Vec<String> {
    let computed = settings.compute();
    computed.iter().filter_map(|stored| written_line(settings, firmware, stored)).collect()
}
