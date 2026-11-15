//! Writing `webcontrol.json` the way `Config.saveSettings` does, so the Python backend reads it unchanged, and the
//! two settings imports (`File/importFile.py`).

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use serde_json::{Map, Value};

use crate::Settings;
use crate::python::python_float_str;

/// The option fields `importWebControlJSON` copies into an option the settings lack.
const IMPORTED_FIELDS: [&str; 9] =
    ["default", "desc", "firmwareKey", "key", "section", "title", "type", "value", "options"];
/// The groundcontrol.ini sections WebControl reads; the others are skipped.
const INI_SECTIONS: [(&str, &str); 6] = [
    ("[Computed Settings]", "Computed Settings"),
    ("[Maslow Settings]", "Maslow Settings"),
    ("[Advanced Settings]", "Advanced Settings"),
    ("[Ground Control Settings]", ""),
    ("[Background Settings]", ""),
    ("[Optical Calibration Settings]", "Optical Calibration Settings"),
];

/// Python's `json.dump(..., sort_keys=True, indent=4, ensure_ascii=False)`.
pub fn python_json(value: &Value) -> String {
    let mut out = String::new();
    write_value(value, 0, &mut out);
    out
}

fn write_value(value: &Value, depth: usize, out: &mut String) {
    let indent = |depth: usize| "    ".repeat(depth);
    match value {
        Value::Number(n) if n.is_f64() => out.push_str(&python_float_str(n.as_f64().unwrap_or_default())),
        Value::Array(items) if !items.is_empty() => {
            out.push_str("[\n");
            let inner: Vec<String> = items
                .iter()
                .map(|item| {
                    let mut text = indent(depth + 1);
                    write_value(item, depth + 1, &mut text);
                    text
                })
                .collect();
            out.push_str(&inner.join(",\n"));
            out.push_str(&format!("\n{}]", indent(depth)));
        }
        Value::Object(map) if !map.is_empty() => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push_str("{\n");
            let inner: Vec<String> = keys
                .iter()
                .map(|key| {
                    let mut text = format!("{}{}: ", indent(depth + 1), Value::String((*key).clone()));
                    write_value(&map[*key], depth + 1, &mut text);
                    text
                })
                .collect();
            out.push_str(&inner.join(",\n"));
            out.push_str(&format!("\n{}}}", indent(depth)));
        }
        other => out.push_str(&other.to_string()),
    }
}

fn add_extension(path: &Path, extension: &str) -> std::path::PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(extension);
    name.into()
}

impl Settings {
    /// `Config.saveSettings`: a temp file, then the old file to `.bak` and the new one in place. The first save ever
    /// also keeps the Python-written file as `.pre-rs`.
    pub fn save_to(&self, file: &Path) -> std::io::Result<()> {
        let snapshot = add_extension(file, ".pre-rs");
        if file.is_file() && !snapshot.exists() {
            fs::copy(file, &snapshot)?;
        }
        if let Some(dir) = file.parent() {
            fs::create_dir_all(dir)?;
        }
        let temp = add_extension(file, ".tmp");
        let written = (|| {
            let mut out = File::create(&temp)?;
            out.write_all(python_json(&Value::Object(self.sections.clone())).as_bytes())?;
            out.sync_all()?;
            if file.exists() {
                fs::rename(file, add_extension(file, ".bak"))?;
            }
            fs::rename(&temp, file)
        })();
        let _ = fs::remove_file(&temp);
        written
    }

    /// Settings from a file's text, as `reloadWebControlJSON` takes a restored one: an object, with the defaults it
    /// lacks added.
    pub fn from_text(text: &str) -> Option<Settings> {
        let sections = crate::settings::parse_object(text)?;
        let mut settings = Settings { sections, source: crate::Source::File };
        settings.add_missing_defaults();
        Some(settings)
    }

    /// `Config.setValue` for an import: the text converted to the option's type; false where Python's conversion
    /// fails or there is no such option.
    pub fn set_typed(&mut self, section: &str, key: &str, text: &str) -> bool {
        let found = self.sections.get_mut(section).and_then(Value::as_array_mut).and_then(|list| {
            list.iter_mut().filter_map(Value::as_object_mut).find(|option| {
                option.get("key").and_then(Value::as_str).is_some_and(|k| k.to_lowercase() == key.to_lowercase())
            })
        });
        let Some(option) = found else { return false };
        let value = match option.get("type").and_then(Value::as_str) {
            Some("float") => text.trim().parse::<f64>().ok().and_then(serde_json::Number::from_f64).map(Value::Number),
            Some("int") => text.trim().parse::<i64>().ok().map(Value::from),
            Some("bool") => Some(Value::from(i64::from(text == "on" || text == "1"))),
            _ => Some(Value::from(text)),
        };
        value.map(|value| option.insert("value".into(), value)).is_some()
    }

    /// `importGCini`: `key = value` lines under the sections WebControl knows; `None` for a file Python refuses.
    pub fn import_ini(&mut self, text: &str) -> Option<()> {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let mut section: Option<&str> = None;
        text.split('\n').try_for_each(|line| {
            if let Some((_, name)) = INI_SECTIONS.iter().find(|(header, _)| *header == line) {
                section = Some(name);
            } else if !line.is_empty() && section? != "" {
                let mut parts = line.split('=').map(str::trim);
                let (key, value) = (parts.next()?, parts.next()?);
                self.set_typed(section?, key, value);
            }
            Some(())
        })
    }

    /// `importWebControlJSON`: changed values replace the stored ones, unknown options are added; whether anything
    /// changed, or `None` for a file Python refuses.
    pub fn import_webcontrol(&mut self, imported: &Value) -> Option<bool> {
        let sections = imported.as_object()?;
        let options: Vec<(&String, &Map<String, Value>)> = sections
            .iter()
            .map(|(name, list)| list.as_array().map(|list| list.iter().map(move |option| (name, option))))
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .flatten()
            .map(|(name, option)| option.as_object().filter(|o| o.contains_key("key")).map(|o| (name, o)))
            .collect::<Option<_>>()?;
        let changed = options.into_iter().filter(|(section, option)| self.import_option(section, option)).count();
        Some(changed > 0)
    }

    fn import_option(&mut self, section: &str, imported: &Map<String, Value>) -> bool {
        let list = self.sections.entry(section).or_insert_with(|| Value::Array(Vec::new()));
        let Value::Array(list) = list else { return false };
        let existing = list.iter_mut().filter_map(Value::as_object_mut).find(|o| o.get("key") == imported.get("key"));
        match (existing, imported.get("value")) {
            (Some(option), Some(value)) if option.get("value") != Some(value) => {
                option.insert("value".into(), value.clone());
                true
            }
            (Some(_), _) => false,
            (None, _) => {
                let copied = imported.iter().filter(|(k, _)| IMPORTED_FIELDS.contains(&k.as_str()));
                list.push(Value::Object(copied.map(|(k, v)| (k.clone(), v.clone())).collect()));
                true
            }
        }
    }
}
