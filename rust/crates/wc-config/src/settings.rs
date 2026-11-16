use serde_json::{Map, Value};

use crate::Paths;

pub(crate) const DEFAULTS: &str = include_str!("../../../../defaultwebcontrol.json");
/// The option fields `Config.addMissingDefaults` copies from the defaults.
const COPIED_FIELDS: [&str; 9] =
    ["default", "desc", "firmwareKey", "key", "section", "title", "type", "value", "options"];
/// What an option the defaults also have takes from them on load: all but its `key` and `value`.
const METADATA_FIELDS: [&str; 7] = ["default", "desc", "firmwareKey", "section", "title", "type", "options"];
const DEFAULT_PORT: u16 = 5000;

/// Which file the settings came from, as `Config.loadSettingsFile` falls back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    File,
    Backup,
    Defaults,
}

/// A setting the controller also holds, as `$<firmware_key>`.
#[derive(Debug, Clone, PartialEq)]
pub struct FirmwareOption {
    pub section: String,
    pub key: String,
    pub firmware_key: u32,
    pub value: Value,
    pub default: Value,
    /// Only custom (optical, version 100+) firmware takes it.
    pub custom: bool,
}

/// `webcontrol.json`: section name -> list of option objects, unknown fields kept as loaded.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub(crate) sections: Map<String, Value>,
    pub(crate) source: Source,
}

pub(crate) fn parse_object(text: &str) -> Option<Map<String, Value>> {
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
        let (mut settings, _) = Settings::read(paths);
        settings.add_missing_defaults();
        settings
    }

    /// The file or its fallback as stored, without the missing defaults.
    pub(crate) fn read(paths: &Paths) -> (Settings, Source) {
        let read = |path: &std::path::Path| std::fs::read_to_string(path).ok().and_then(|text| parse_object(&text));
        let (sections, source) = read(&paths.settings_file())
            .map(|s| (s, Source::File))
            .or_else(|| read(&paths.settings_backup()).map(|s| (s, Source::Backup)))
            .unwrap_or_else(|| (defaults(), Source::Defaults));
        (Settings { sections, source }, source)
    }

    pub fn defaults() -> Settings {
        Settings { sections: defaults(), source: Source::Defaults }
    }

    /// Every field but `value` of an option the defaults also have, as the defaults now give it.
    fn refresh_metadata(&mut self, defaults: &Map<String, Value>) {
        self.sections.iter_mut().for_each(|(name, section)| {
            let Some(list) = section.as_array_mut() else { return };
            list.iter_mut().filter_map(Value::as_object_mut).for_each(|option| {
                let found = defaults.get(name).and_then(|s| options(s).find(|d| d.get("key") == option.get("key")));
                let Some(default) = found else { return };
                METADATA_FIELDS.iter().for_each(|field| {
                    match default.get(*field) {
                        Some(value) => option.insert((*field).into(), value.clone()),
                        None => option.remove(*field),
                    };
                });
            });
        });
    }

    /// True if any option was added. Options already there get the defaults' metadata, which alone never makes a
    /// save: the file keeps what it had until a value changes.
    pub(crate) fn add_missing_defaults(&mut self) -> bool {
        let defaults = defaults();
        self.refresh_metadata(&defaults);
        let mut added = false;
        for (name, section) in defaults {
            let missing: Vec<Value> =
                options(&section).filter(|o| !has_key(self.sections.get(&name), &o["key"])).map(copied).collect();
            if missing.is_empty() {
                continue;
            }
            added = true;
            let target = self.sections.entry(name).or_insert_with(|| Value::Array(Vec::new()));
            match target {
                Value::Array(list) => list.extend(missing),
                other => *other = Value::Array(missing),
            }
        }
        added
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

    /// The option objects of one section, in file order.
    pub fn section(&self, section: &str) -> Vec<&Map<String, Value>> {
        self.sections.get(section).map(|s| options(s).collect()).unwrap_or_default()
    }

    /// Every option with a `firmwareKey`, in file order.
    pub fn firmware_options(&self) -> impl Iterator<Item = FirmwareOption> + '_ {
        self.sections.iter().flat_map(|(name, section)| {
            options(section).filter_map(move |option| {
                Some(FirmwareOption {
                    section: name.clone(),
                    key: option.get("key")?.as_str()?.to_string(),
                    firmware_key: u32::try_from(option.get("firmwareKey")?.as_u64()?).ok()?,
                    value: option.get("value").cloned().unwrap_or(Value::Null),
                    default: option.get("default").cloned().unwrap_or(Value::Null),
                    custom: option.get("custom") == Some(&Value::from(1)),
                })
            })
        })
    }

    /// `Config.setValue` without its side effects; false if the option does not exist.
    pub fn set_value(&mut self, section: &str, key: &str, value: Value) -> bool {
        let found = self.sections.get_mut(section).and_then(Value::as_array_mut).and_then(|list| {
            list.iter_mut().filter_map(Value::as_object_mut).find(|o| o.get("key").and_then(Value::as_str) == Some(key))
        });
        found.map(|option| option.insert("value".into(), value)).is_some()
    }

    pub fn web_port(&self) -> u16 {
        let port = self.number("WebControl Settings", "webPort");
        if (1.0..=65535.0).contains(&port) { port as u16 } else { DEFAULT_PORT }
    }
}
