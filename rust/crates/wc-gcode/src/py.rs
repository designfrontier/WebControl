//! The Python built-ins the Python backend's gcode code leans on, reproduced exactly where its output reaches the
//! controller or a file Python reads back.

use serde_json::Value;

/// Python's `str.isspace` for one character, which also counts the ASCII separators 0x1C–0x1F.
pub fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)
}

/// Python's `str.isspace`: true for a non-empty string of whitespace only.
pub fn isspace(text: &str) -> bool {
    !text.is_empty() && text.chars().all(is_space)
}

pub fn strip(text: &str) -> &str {
    text.trim_matches(is_space)
}

pub fn lstrip(text: &str) -> &str {
    text.trim_start_matches(is_space)
}

/// Python's `str.split()` with no separator.
pub fn split(text: &str) -> impl Iterator<Item = &str> {
    text.split(is_space).filter(|part| !part.is_empty())
}

/// Python's `str.upper`.
pub fn upper(text: &str) -> String {
    text.to_uppercase()
}

/// Drops the underscores Python allows between digits, or refuses a misplaced one.
fn without_underscores(text: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let misplaced = chars.iter().enumerate().any(|(index, &c)| {
        c == '_'
            && !(index > 0
                && chars[index - 1].is_ascii_digit()
                && chars.get(index + 1).is_some_and(|next| next.is_ascii_digit()))
    });
    (!misplaced).then(|| chars.into_iter().filter(|&c| c != '_').collect())
}

/// Python's `float(text)`.
pub fn float(text: &str) -> Option<f64> {
    let cleaned = without_underscores(strip(text))?;
    let unsigned = cleaned.trim_start_matches(['+', '-']);
    let finite = unsigned.chars().next().is_some_and(|c| c.is_ascii_digit() || c == '.');
    let named = ["inf", "infinity", "nan"].iter().any(|name| unsigned.eq_ignore_ascii_case(name));
    let one_sign = cleaned.len() - unsigned.len() <= 1;
    if !(one_sign && (finite || named)) {
        return None;
    }
    cleaned.parse().ok()
}

/// Python's `int(text)` for values that fit an `i64`.
pub fn int(text: &str) -> Option<i64> {
    let cleaned = without_underscores(strip(text))?;
    let digits = cleaned.strip_prefix(['+', '-']).unwrap_or(&cleaned);
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    cleaned.parse().ok()
}

/// Python's `round(value, digits)`: the correctly rounded decimal, ties to even, read back as a float.
pub fn round(value: f64, digits: usize) -> f64 {
    if !value.is_finite() {
        return value;
    }
    format!("{value:.digits$}").parse().expect("a formatted float parses")
}

/// Python's `round(value)`, which gives an int, ties to even; `None` where Python raises.
pub fn round_int(value: f64) -> Option<i64> {
    value.is_finite().then(|| value.round_ties_even() as i64)
}

/// Python's `repr(float)`, which is also what `str()` gives.
pub fn repr(value: f64) -> String {
    if value.is_nan() {
        return "nan".into();
    }
    if value.is_infinite() {
        return if value > 0.0 { "inf".into() } else { "-inf".into() };
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

/// A JSON string as Python's `json.dumps` writes it, with `ensure_ascii`.
pub fn json_string(text: &str) -> String {
    let body: String = text
        .chars()
        .map(|c| match c {
            '"' => "\\\"".to_string(),
            '\\' => "\\\\".to_string(),
            '\n' => "\\n".to_string(),
            '\r' => "\\r".to_string(),
            '\t' => "\\t".to_string(),
            '\x08' => "\\b".to_string(),
            '\x0c' => "\\f".to_string(),
            ' '..='~' => c.to_string(),
            _ => c.encode_utf16(&mut [0; 2]).iter().map(|unit| format!("\\u{unit:04x}")).collect(),
        })
        .collect();
    format!("\"{body}\"")
}

/// A JSON value as Python's `json.dumps` writes it, for the scalars and arrays a board file holds.
pub fn json_value(value: &Value) -> String {
    match value {
        Value::Number(number) if number.is_f64() => json_float(number.as_f64().unwrap_or_default()),
        Value::String(text) => json_string(text),
        Value::Array(items) => format!("[{}]", items.iter().map(json_value).collect::<Vec<_>>().join(", ")),
        other => other.to_string(),
    }
}

fn json_float(value: f64) -> String {
    match repr(value).as_str() {
        "nan" => "NaN".into(),
        "inf" => "Infinity".into(),
        "-inf" => "-Infinity".into(),
        text => text.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn whitespace_includes_the_ascii_separators() {
        assert!(isspace("\n \x1c\x1f\u{a0}"));
        assert!(!isspace(""));
        assert!(!isspace(" x "));
        assert_eq!(strip("\x1e G1 \t"), "G1");
        assert_eq!(lstrip("  G1 "), "G1 ");
        assert_eq!(split(" a  b\x1fc ").collect::<Vec<_>>(), ["a", "b", "c"]);
        assert_eq!(upper("g1 x1"), "G1 X1");
    }

    #[test]
    fn float_accepts_what_python_accepts() {
        let accepted = [
            (" 5", 5.0),
            ("+5", 5.0),
            ("5.", 5.0),
            (".5", 0.5),
            ("-.5", -0.5),
            ("1e3", 1000.0),
            ("1_000", 1000.0),
            ("1000 ", 1000.0),
            ("Infinity", f64::INFINITY),
        ];
        accepted.iter().for_each(|(text, value)| assert_eq!(float(text), Some(*value), "{text}"));
        assert!(float("nan").unwrap().is_nan());
        ["", " ", ".", "-", "+-5", "--5", "1_", "_1", "1__0", "M3 ", "1000 M8", "0x10", "e5"]
            .iter()
            .for_each(|text| assert_eq!(float(text), None, "{text}"));
    }

    #[test]
    fn int_accepts_what_python_accepts() {
        assert_eq!(int("2 "), Some(2));
        assert_eq!(int(" -3"), Some(-3));
        assert_eq!(int("1_0"), Some(10));
        ["", "2.0", "x", "+", "99999999999999999999"].iter().for_each(|text| assert_eq!(int(text), None, "{text}"));
    }

    #[test]
    fn round_matches_python_on_exact_ties_and_binary_near_ties() {
        assert_eq!(round(0.125, 2), 0.12);
        assert_eq!(round(0.375, 2), 0.38);
        assert_eq!(round(2.675, 2), 2.67);
        assert_eq!(round(2.5, 0), 2.0);
        assert_eq!(round(5.0 / 25.4, 4), 0.1969);
        assert_eq!(repr(round(-0.00001, 4)), "-0.0");
        assert!(round(f64::NAN, 2).is_nan());
        assert_eq!(round_int(2.5), Some(2));
        assert_eq!(round_int(-3.5), Some(-4));
        assert_eq!(round_int(f64::INFINITY), None);
    }

    #[test]
    fn repr_matches_python() {
        let cases = [
            (1.0, "1.0"),
            (12.8, "12.8"),
            (0.1 + 0.2, "0.30000000000000004"),
            (1e-5, "1e-05"),
            (0.0001, "0.0001"),
            (1e16, "1e+16"),
            (1.5e300, "1.5e+300"),
            (123456789012345.6, "123456789012345.6"),
            (-0.0, "-0.0"),
            (f64::NAN, "nan"),
            (f64::INFINITY, "inf"),
            (f64::NEG_INFINITY, "-inf"),
        ];
        cases.iter().for_each(|(value, text)| assert_eq!(repr(*value), *text));
    }

    #[test]
    fn json_matches_python_dumps() {
        let expected = r#""~u00fc~u00f1\"\\\n\r\t\b\f~u0001~ud83d~ude00""#.replace('~', "\\");
        assert_eq!(json_string("üñ\"\\\n\r\t\x08\x0c\x01😀"), expected);
        assert_eq!(json_value(&json!([1, 2.0, "a", true, null, [0.5]])), r#"[1, 2.0, "a", true, null, [0.5]]"#);
        assert_eq!(json_float(f64::NAN), "NaN");
        assert_eq!(json_float(f64::INFINITY), "Infinity");
        assert_eq!(json_float(f64::NEG_INFINITY), "-Infinity");
    }
}
