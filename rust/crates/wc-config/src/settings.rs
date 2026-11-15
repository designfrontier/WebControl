use serde_json::{Map, Value};

use crate::Paths;

const DEFAULTS: &str = include_str!("../../../../defaultwebcontrol.json");
/// The option fields `Config.addMissingDefaults` copies from the defaults.
const COPIED_FIELDS: [&str; 9] =
    ["default", "desc", "firmwareKey", "key", "section", "title", "type", "value", "options"];
const DEFAULT_PORT: u16 = 5000;

/// Which file the settings came from, as `Config.loadSettingsFile` falls back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    File,
    Backup,
    Defaults,
}

/// `webcontrol.json`: section name -> list of option objects, unknown fields kept as loaded.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    sections: Map<String, Value>,
    source: Source,
}

fn parse_object(text: &str) -> Option<Map<String, Value>> {
    match serde_json::from_str(text) {
        Ok(Value::Object(map)) => Some(map),
        _ => None,
    }
}

fn defaults() -> Map<String, Value> {
    parse_object(DEFAULTS).expect("defaultwebcontrol.json is an object")
}

fn options(section: &Value) -> impl Iterator<Item = &Map<String, Value>> {
    section.as_array().into_iter().flatten().filter_map(Value::as_object)
}

fn has_key(section: Option<&Value>, key: &Value) -> bool {
    section.is_some_and(|s| options(s).any(|option| option.get("key") == Some(key)))
}

fn copied(option: &Map<String, Value>) -> Value {
    Value::Object(
        option
            .iter()
            .filter(|(k, _)| COPIED_FIELDS.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    )
}

fn falsy(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Bool(b) => !b,
        Value::Number(n) => n.as_f64() == Some(0.0),
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
    }
}

impl Settings {
    /// `webcontrol.json`, else its `.bak`, else the defaults; then any option the defaults have and it lacks.
    pub fn load(paths: &Paths) -> Settings {
        let file = paths.settings_file();
        let backup = file.with_extension("json.bak");
        let read = |path: &std::path::Path| std::fs::read_to_string(path).ok().and_then(|text| parse_object(&text));
        let (sections, source) = read(&file)
            .map(|s| (s, Source::File))
            .or_else(|| read(&backup).map(|s| (s, Source::Backup)))
            .unwrap_or_else(|| (defaults(), Source::Defaults));
        let mut settings = Settings { sections, source };
        settings.add_missing_defaults();
        settings
    }

    pub fn defaults() -> Settings {
        Settings { sections: defaults(), source: Source::Defaults }
    }

    fn add_missing_defaults(&mut self) {
        for (name, section) in defaults() {
            let missing: Vec<Value> =
                options(&section).filter(|o| !has_key(self.sections.get(&name), &o["key"])).map(copied).collect();
            if missing.is_empty() {
                continue;
            }
            let target = self.sections.entry(name).or_insert_with(|| Value::Array(Vec::new()));
            match target {
                Value::Array(list) => list.extend(missing),
                other => *other = Value::Array(missing),
            }
        }
    }

    pub fn source(&self) -> Source {
        self.source
    }

    pub fn option(&self, section: &str, key: &str) -> Option<&Map<String, Value>> {
        options(self.sections.get(section)?).find(|o| o.get("key").and_then(Value::as_str) == Some(key))
    }

    /// `Config.getValue`.
    pub fn value(&self, section: &str, key: &str) -> Option<&Value> {
        self.option(section, key)?.get("value")
    }

    pub fn text(&self, section: &str, key: &str) -> Option<String> {
        self.value(section, key).map(crate::python_str)
    }

    /// `str(getValue(...) or "")`.
    pub fn text_or_empty(&self, section: &str, key: &str) -> String {
        self.value(section, key).filter(|v| !falsy(v)).map(crate::python_str).unwrap_or_default()
    }

    /// `float(getValue(...))`, or 0.0 where Python would raise; never NaN or infinite, which JSON cannot carry.
    pub fn number(&self, section: &str, key: &str) -> f64 {
        let parsed = match self.value(section, key) {
            Some(Value::Number(n)) => n.as_f64(),
            Some(Value::String(s)) => s.trim().parse::<f64>().ok(),
            Some(Value::Bool(b)) => Some(f64::from(u8::from(*b))),
            _ => None,
        };
        parsed.filter(|n| n.is_finite()).unwrap_or(0.0)
    }

    pub fn web_port(&self) -> u16 {
        let port = self.number("WebControl Settings", "webPort");
        if (1.0..=65535.0).contains(&port) { port as u16 } else { DEFAULT_PORT }
    }
}
