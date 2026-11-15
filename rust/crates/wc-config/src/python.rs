use serde_json::Value;

/// Python's `str(float)`.
pub fn python_float_str(value: f64) -> String {
    if value.is_nan() {
        return "nan".into();
    }
    if value.is_infinite() {
        return if value > 0.0 { "inf" } else { "-inf" }.into();
    }
    let scientific = format!("{value:e}");
    let (mantissa, exponent) = scientific.split_once('e').expect("{:e} always has an exponent");
    let exponent: i32 = exponent.parse().expect("integer exponent");
    if !(-4..16).contains(&exponent) {
        let sign = if exponent < 0 { '-' } else { '+' };
        return format!("{mantissa}e{sign}{:02}", exponent.abs());
    }
    let fixed = value.to_string();
    if fixed.contains('.') { fixed } else { format!("{fixed}.0") }
}

/// What Python's `str()` prints for a value loaded from JSON.
pub fn python_str(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(n) if n.is_f64() => python_float_str(n.as_f64().unwrap_or_default()),
        Value::Number(n) => n.to_string(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Null => "None".into(),
        other => other.to_string(),
    }
}

fn is_decimal(text: &str) -> bool {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    let digits = |part: &str| part.chars().all(|c| c.is_ascii_digit());
    digits(whole) && digits(fraction) && !(whole.is_empty() && fraction.is_empty())
}

/// A number in the form the frontend and `api/settings.py` accept: no `nan`, `inf` or `1_000`.
pub fn parse_number(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    let unsigned = trimmed.strip_prefix(['+', '-']).unwrap_or(trimmed);
    let (mantissa, exponent) = match unsigned.find(['e', 'E']) {
        Some(at) => (&unsigned[..at], Some(&unsigned[at + 1..])),
        None => (unsigned, None),
    };
    let exponent_ok = exponent.is_none_or(|e| {
        let digits = e.strip_prefix(['+', '-']).unwrap_or(e);
        !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
    });
    if !is_decimal(mantissa) || !exponent_ok {
        return None;
    }
    trimmed.parse::<f64>().ok().filter(|v| v.is_finite())
}
