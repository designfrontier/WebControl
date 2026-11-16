//! A restored `webcontrol.json` and the two settings imports (`File/importFile.py`).

use serde_json::{Map, Value};

use crate::Settings;

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
/// The error array is a comma-separated list, not a number.
const ERROR_ARRAY_KEY: u64 = 85;

/// How an import took one value.
#[derive(Debug, Clone, PartialEq)]
pub enum Taken {
    Stored,
    /// Stored after its decimal comma became a point; the note says so.
    Converted(String),
    /// The current value is kept; the note says why.
    Refused(String),
    /// There is no such option.
    Unknown,
}

impl Taken {
    pub fn note(self) -> Option<String> {
        match self {
            Taken::Converted(note) | Taken::Refused(note) => Some(note),
            Taken::Stored | Taken::Unknown => None,
        }
    }
}

/// What an import changed, and what the user should hear about the values it converted or refused.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Imported {
    pub changed: bool,
    pub notes: Vec<String>,
}

/// A bool as GroundControl and hand-edited files write it; deliberately wider than Python's `"on"`/`"1"`.
fn bool_text(text: &str) -> Option<bool> {
    match text.to_lowercase().as_str() {
        "true" | "on" | "yes" => Some(true),
        "false" | "off" | "no" => Some(false),
        number => crate::parse_number(number).map(|n| n != 0.0),
    }
}

/// The text as a decimal with a point, and whether a decimal comma was converted; or why it is no number.
fn decimal_text(text: &str) -> Result<(String, bool), String> {
    if crate::parse_number(text).is_some() {
        return Ok((text.to_string(), false));
    }
    let Some((whole, fraction)) = text.split_once(',').filter(|_| !text.contains('.')) else {
        return Err("is not a number".into());
    };
    let pointed = format!("{whole}.{fraction}");
    if crate::parse_number(&pointed).is_none() {
        return Err("is not a number".into());
    }
    let digits = whole.trim_start_matches(['+', '-']);
    let grouped = fraction.len() == 3 && (1..=3).contains(&digits.len()) && !digits.starts_with('0');
    match grouped {
        true => Err(format!("could be {pointed} or {whole}{fraction}; write it with a point")),
        false => Ok((pointed, true)),
    }
}

fn firmware_key(option: &Map<String, Value>) -> Option<u64> {
    option.get("firmwareKey").and_then(Value::as_u64).filter(|key| *key != ERROR_ARRAY_KEY)
}

/// A text setting that holds a number: its default is one, or the controller holds it.
pub(crate) fn is_numeric_text(option: &Map<String, Value>) -> bool {
    let kind = option.get("type").and_then(Value::as_str).unwrap_or("string");
    let default_is_number = match option.get("default") {
        Some(Value::Number(_)) => true,
        Some(Value::String(text)) => crate::parse_number(text).is_some(),
        _ => false,
    };
    kind == "string" && (default_is_number || firmware_key(option).is_some())
}

/// Blank means "not read from the controller yet" for a controller setting.
fn blank_allowed(option: &Map<String, Value>) -> bool {
    firmware_key(option).is_some() || option.get("default").is_none_or(|d| d == "" || d.is_null())
}

fn is_numeric(option: &Map<String, Value>) -> bool {
    matches!(option.get("type").and_then(Value::as_str), Some("float" | "int")) || is_numeric_text(option)
}

impl Settings {
    /// Settings from a file's text, as `reloadWebControlJSON` takes a restored one: an object, with the defaults it
    /// lacks added.
    pub fn from_text(text: &str) -> Option<Settings> {
        let sections = crate::settings::parse_object(text)?;
        let mut settings = Settings { sections, source: crate::Source::File };
        settings.add_missing_defaults();
        Some(settings)
    }

    /// `Config.setValue` for an import: the text converted to the option's type. Unlike Python, a bool takes any
    /// non-zero number as on, and text that is neither a bool nor a number keeps the current value.
    pub fn set_typed(&mut self, section: &str, key: &str, text: &str) -> Taken {
        let found = self.sections.get_mut(section).and_then(Value::as_array_mut).and_then(|list| {
            list.iter_mut().filter_map(Value::as_object_mut).find(|option| {
                option.get("key").and_then(Value::as_str).is_some_and(|k| k.to_lowercase() == key.to_lowercase())
            })
        });
        let Some(option) = found else { return Taken::Unknown };
        let name = option.get("key").and_then(Value::as_str).unwrap_or(key).to_string();
        let text = text.trim();
        let kept = |why: &str| Taken::Refused(format!("Kept {name} as it was: \"{text}\" {why}."));
        let (value, converted) = match option.get("type").and_then(Value::as_str) {
            Some("bool") => match bool_text(text) {
                Some(on) => (Value::from(i64::from(on)), None),
                None => return kept("is neither on nor off"),
            },
            Some(kind @ ("float" | "int")) => {
                let (decimal, converted) = match decimal_text(text) {
                    Ok(decimal) => decimal,
                    Err(why) => return kept(&why),
                };
                let value = match kind {
                    "int" => decimal.parse::<i64>().ok().map(Value::from),
                    _ => decimal.parse::<f64>().ok().and_then(serde_json::Number::from_f64).map(Value::Number),
                };
                let Some(value) = value else { return kept("is not a whole number") };
                (value, converted.then_some(decimal))
            }
            _ if text.is_empty() && blank_allowed(option) => (Value::from(text), None),
            _ if is_numeric_text(option) => match decimal_text(text) {
                Ok((decimal, converted)) => (Value::from(decimal.clone()), converted.then_some(decimal)),
                Err(why) => return kept(&why),
            },
            _ => (Value::from(text), None),
        };
        option.insert("value".into(), value);
        match converted {
            Some(decimal) => Taken::Converted(format!("Read {name} \"{text}\" as {decimal}.")),
            None => Taken::Stored,
        }
    }

    /// `importGCini`: `key = value` lines under the sections WebControl knows; `None` for a file Python refuses.
    pub fn import_ini(&mut self, text: &str) -> Option<Imported> {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let mut section: Option<&str> = None;
        let mut notes = Vec::new();
        text.split('\n').try_for_each(|line| {
            if let Some((_, name)) = INI_SECTIONS.iter().find(|(header, _)| *header == line) {
                section = Some(name);
            } else if !line.is_empty() && !section?.is_empty() {
                let mut parts = line.split('=').map(str::trim);
                let (key, value) = (parts.next()?, parts.next()?);
                notes.extend(self.set_typed(section?, key, value).note());
            }
            Some(())
        })?;
        Some(Imported { changed: true, notes })
    }

    /// `importWebControlJSON`: changed values replace the stored ones, unknown options are added; what changed, or
    /// `None` for a file Python refuses. Text for a numeric setting is checked as the ini import checks it.
    pub fn import_webcontrol(&mut self, imported: &Value) -> Option<Imported> {
        let sections = imported.as_object()?;
        let options: Vec<(&String, &Map<String, Value>)> = sections
            .iter()
            .map(|(name, list)| list.as_array().map(|list| list.iter().map(move |option| (name, option))))
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .flatten()
            .map(|(name, option)| option.as_object().filter(|o| o.contains_key("key")).map(|o| (name, o)))
            .collect::<Option<_>>()?;
        let mut notes = Vec::new();
        let changed = options.into_iter().filter(|(section, option)| self.import_option(section, option, &mut notes));
        let changed = changed.count() > 0;
        Some(Imported { changed, notes })
    }

    fn import_option(&mut self, section: &str, imported: &Map<String, Value>, notes: &mut Vec<String>) -> bool {
        let list = self.sections.entry(section).or_insert_with(|| Value::Array(Vec::new()));
        let Value::Array(list) = list else { return false };
        let existing = list.iter_mut().filter_map(Value::as_object_mut).find(|o| o.get("key") == imported.get("key"));
        match (existing, imported.get("value")) {
            (Some(option), Some(value)) => {
                let value = match value.as_str().filter(|text| is_numeric(option) && !text.is_empty()) {
                    None => value.clone(),
                    Some(text) => {
                        let name = imported.get("key").map(crate::python_str).unwrap_or_default();
                        match decimal_text(text.trim()) {
                            Ok((_, false)) => value.clone(),
                            Ok((decimal, true)) => {
                                notes.push(format!("Read {name} \"{text}\" as {decimal}."));
                                Value::from(decimal)
                            }
                            Err(why) => {
                                notes.push(format!("Kept {name} as it was: \"{text}\" {why}."));
                                return false;
                            }
                        }
                    }
                };
                option.get("value").is_none_or(|current| *current != value) && {
                    option.insert("value".into(), value);
                    true
                }
            }
            (Some(_), None) => false,
            (None, _) => {
                let copied = imported.iter().filter(|(k, _)| IMPORTED_FIELDS.contains(&k.as_str()));
                list.push(Value::Object(copied.map(|(k, v)| (k.clone(), v.clone())).collect()));
                true
            }
        }
    }
}
