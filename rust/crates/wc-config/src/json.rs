use serde_json::Value;

use crate::python_float_str;

const INDENT: &str = "    ";

/// `json.dump(value, sort_keys=True, indent=4, ensure_ascii=False)`, byte for byte.
pub fn python_json(value: &Value) -> String {
    let mut out = String::new();
    write(&mut out, value, 0);
    out
}

fn quoted(text: &str) -> String {
    serde_json::to_string(text).expect("a string always serialises")
}

fn write(out: &mut String, value: &Value, depth: usize) {
    match value {
        Value::Number(n) if n.is_f64() => out.push_str(&python_float_str(n.as_f64().unwrap_or_default())),
        Value::String(text) => out.push_str(&quoted(text)),
        Value::Array(items) if !items.is_empty() => {
            container(out, '[', ']', depth, items.iter().map(|item| (None, item)));
        }
        // serde_json's map is a BTreeMap, which orders keys by code point as sort_keys does.
        Value::Object(map) if !map.is_empty() => {
            container(out, '{', '}', depth, map.iter().map(|(key, item)| (Some(key.as_str()), item)));
        }
        other => out.push_str(&other.to_string()),
    }
}

fn container<'a>(
    out: &mut String,
    open: char,
    close: char,
    depth: usize,
    items: impl Iterator<Item = (Option<&'a str>, &'a Value)>,
) {
    out.push(open);
    items.enumerate().for_each(|(i, (key, item))| {
        out.push_str(if i == 0 { "\n" } else { ",\n" });
        out.push_str(&INDENT.repeat(depth + 1));
        if let Some(key) = key {
            out.push_str(&quoted(key));
            out.push_str(": ");
        }
        write(out, item, depth + 1);
    });
    out.push('\n');
    out.push_str(&INDENT.repeat(depth));
    out.push(close);
}
