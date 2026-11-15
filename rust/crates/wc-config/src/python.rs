use serde_json::Value;

/// Python's `repr(float)` for the finite values JSON can carry.
fn float_repr(value: f64) -> String {
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
        Value::Number(n) if n.is_f64() => float_repr(n.as_f64().unwrap_or_default()),
        Value::Number(n) => n.to_string(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Null => "None".into(),
        other => other.to_string(),
    }
}
