use serde_json::Value;
use wc_kinematics::python_float;

/// Python's `round(value, digits)`: the nearest decimal with that many places, from the exact binary value.
pub fn round(value: f64, digits: usize) -> f64 {
    format!("{value:.digits$}").parse().unwrap_or(value)
}

/// Python's `float(value)` for a value from a JSON form.
pub fn float(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => python_float(text),
        Value::Bool(flag) => Some(if *flag { 1.0 } else { 0.0 }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn round_matches_python() {
        assert_eq!(round(2990.000000031, 5), 2990.0);
        assert_eq!(round(0.123456789, 6), 0.123457);
        assert_eq!(round(-0.000004, 5), -0.0);
        assert_eq!(round(2.675, 2), 2.67);
        assert!(round(f64::NAN, 5).is_nan());
        assert_eq!(round(f64::INFINITY, 1), f64::INFINITY);
    }

    #[test]
    fn float_matches_python() {
        assert_eq!(float(&json!(3)), Some(3.0));
        assert_eq!(float(&json!(" 2.5 ")), Some(2.5));
        assert_eq!(float(&json!(true)), Some(1.0));
        assert_eq!(float(&json!(false)), Some(0.0));
        assert_eq!(float(&json!("abc")), None);
        assert_eq!(float(&json!(null)), None);
        assert_eq!(float(&json!([1])), None);
    }
}
