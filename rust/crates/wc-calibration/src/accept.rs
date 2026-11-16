use serde_json::Value;
use wc_firmware::settings::setting_line;
use wc_kinematics::python_float;

use crate::CalibrationError;

/// One `config.setValue(section, key, value)` an Accept makes, with the value as Python's `str()` prints it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingWrite {
    pub section: &'static str,
    pub key: &'static str,
    pub value: String,
}

/// What pressing Accept does: the settings it saves, in order, then the G-code it queues after them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accept {
    pub writes: Vec<SettingWrite>,
    pub gcode: Vec<String>,
}

fn firmware_key(key: &str) -> Option<u32> {
    match key {
        "motorSpacingX" => Some(2),
        "motorOffsetY" => Some(3),
        "rotationRadius" => Some(8),
        "chainSagCorrection" => Some(37),
        "leftChainTolerance" => Some(40),
        "rightChainTolerance" => Some(41),
        _ => None,
    }
}

/// `config.isClose`: equal to within one part in a million.
fn is_close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * a.abs().max(b.abs())
}

/// `firmwareValuesMatch` for a value the user saved: an empty new value never syncs, an empty old one always does.
fn values_match(new: &str, previous: Option<&str>) -> bool {
    if new.is_empty() {
        return true;
    }
    match (python_float(new), previous.and_then(python_float)) {
        (Some(new), Some(previous)) => is_close(new, previous),
        _ => false,
    }
}

fn word_is_finite(word: &str) -> bool {
    word.get(1..).and_then(|number| number.parse::<f64>().ok()).is_none_or(f64::is_finite)
}

impl Accept {
    /// Defence in depth: the calculations already refuse NaN and infinity, but nothing here may save or send one.
    pub fn check(&self) -> Result<(), CalibrationError> {
        let finite = |value: &str| value.is_empty() || python_float(value).is_some_and(f64::is_finite);
        let values = self.writes.iter().all(|write| finite(&write.value));
        let gcode = self.gcode.iter().flat_map(|line| line.split_whitespace()).all(word_is_finite);
        if values && gcode { Ok(()) } else { Err(CalibrationError::NotFinite) }
    }

    /// Every line the Accept puts on the G-code queue: a `$key=value` for each firmware setting whose value
    /// changes from what `previous(section, key)` holds, then the trailing G-code.
    pub fn queued(&self, previous: impl Fn(&str, &str) -> Option<String>) -> Result<Vec<String>, CalibrationError> {
        self.check()?;
        Ok(self
            .writes
            .iter()
            .filter_map(|write| {
                let key = firmware_key(write.key)?;
                let unchanged = values_match(&write.value, previous(write.section, write.key).as_deref());
                (!unchanged).then(|| setting_line(key, &Value::String(write.value.clone())))
            })
            .chain(self.gcode.iter().cloned())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(key: &'static str, value: &str) -> SettingWrite {
        SettingWrite { section: "Maslow Settings", key, value: value.into() }
    }

    #[test]
    fn only_changed_firmware_settings_are_sent() {
        let accept = Accept {
            writes: vec![
                write("motorOffsetY", "463.0000001"),
                write("motorSpacingX", "2990.0"),
                write("sledWeight", "100"),
                write("leftChainTolerance", ""),
                write("rightChainTolerance", "0.1"),
                write("rotationRadius", "139"),
            ],
            gcode: vec!["G21 ".into()],
        };
        let previous = |_: &str, key: &str| match key {
            "motorOffsetY" => Some("463".to_string()),
            "motorSpacingX" => Some("2978.4".to_string()),
            "rightChainTolerance" => Some("".to_string()),
            _ => None,
        };
        assert_eq!(
            accept.queued(previous),
            Ok(vec!["$2=2990".into(), "$41=0.1".into(), "$8=139".into(), "G21 ".into()])
        );
    }

    #[test]
    fn a_value_that_is_not_finite_refuses_the_whole_accept() {
        let refused = |writes: Vec<SettingWrite>, gcode: &[&str]| {
            let accept = Accept { writes, gcode: gcode.iter().map(|line| line.to_string()).collect() };
            (accept.check(), accept.queued(|_, _| None))
        };
        let expected = (Err(CalibrationError::NotFinite), Err(CalibrationError::NotFinite));
        assert_eq!(refused(vec![write("motorOffsetY", "nan")], &[]), expected);
        assert_eq!(
            refused(vec![write("sledWeight", "-inf")], &[]),
            expected,
            "even a setting the controller never gets"
        );
        assert_eq!(refused(vec![write("motorOffsetY", "470.0")], &["G0 Zinf "]), expected);
        assert_eq!(refused(vec![write("motorOffsetY", "1e999")], &[]), expected);
        let fine =
            Accept { writes: vec![write("leftChainTolerance", "")], gcode: vec!["G0 Z5.0 ".into(), "G21".into()] };
        assert_eq!(fine.check(), Ok(()));
        assert_eq!(
            CalibrationError::NotFinite.to_string(),
            "The calibration result is not a finite number, so it was not saved. Calculate it again."
        );
    }

    #[test]
    fn is_close_is_relative() {
        assert!(is_close(1000.0, 1000.0009));
        assert!(!is_close(1000.0, 1000.002));
        assert!(is_close(0.0, 0.0));
    }
}
