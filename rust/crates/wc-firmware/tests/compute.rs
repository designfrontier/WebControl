//! `computeSettings` against fixtures/compute/cases.json: what the Python backend derived, and the `$` lines it sent,
//! for varied webcontrol.json files and groundcontrol.ini imports. The file is frozen; edit it by hand.

use std::path::PathBuf;

use serde_json::{Map, Value, json};
use wc_config::Settings;
use wc_firmware::settings::recompute;

const FIRMWARE: f64 = 51.27;
const DEFAULTS: &str = include_str!("../../../../defaultwebcontrol.json");

fn cases() -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures/compute/cases.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The defaults with the case's values written over them, loaded as `Config.__init__` loads a file.
fn opened(overrides: &Map<String, Value>) -> Settings {
    let mut file: Value = serde_json::from_str(DEFAULTS).unwrap();
    overrides.iter().for_each(|(section, values)| {
        file[section].as_array_mut().unwrap().iter_mut().for_each(|option| {
            if let Some(value) = values.get(option["key"].as_str().unwrap()) {
                option["value"] = value.clone();
            }
        });
    });
    Settings::from_text(&file.to_string()).unwrap()
}

fn as_json(settings: &Settings) -> Value {
    serde_json::from_str(&settings.to_python_json()).unwrap()
}

/// Every option value that differs, JSON type included, as sorted `[section, key, value]`.
fn changes(before: &Value, after: &Value) -> Vec<Value> {
    let values = |settings: &Value| -> Vec<(String, String, Value)> {
        let sections = settings.as_object().unwrap().iter();
        let mut all: Vec<_> = sections
            .flat_map(|(section, options)| {
                options.as_array().unwrap().iter().map(move |option| {
                    (section.clone(), option["key"].as_str().unwrap().to_string(), option["value"].clone())
                })
            })
            .collect();
        all.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
        all
    };
    let old = values(before);
    values(after)
        .into_iter()
        .filter(|(section, key, value)| !old.iter().any(|(s, k, v)| s == section && k == key && v == value))
        .map(|(section, key, value)| json!([section, key, value]))
        .collect()
}

fn derived(settings: &mut Settings, change: impl FnOnce(&mut Settings)) -> Value {
    let before = as_json(settings);
    change(settings);
    let lines = recompute(settings, FIRMWARE);
    json!({ "changes": changes(&before, &as_json(settings)), "lines": lines })
}

#[test]
fn the_derived_settings_and_lines_match_python_for_every_case() {
    let cases = cases();
    assert!(cases.len() >= 10);
    cases.iter().for_each(|case| {
        let name = case["name"].as_str().unwrap();
        let mut settings = opened(case["overrides"].as_object().unwrap());
        assert_eq!(derived(&mut settings, |_| ()), case["startup"], "{name} at startup");
        if let Some(ini) = case["ini"].as_str() {
            let imported = derived(&mut settings, |settings| settings.import_ini(ini).unwrap());
            assert_eq!(imported, case["import"], "{name} after the groundcontrol.ini import");
        }
    });
}

#[test]
fn the_owner_like_settings_are_among_the_cases() {
    let owner = cases().into_iter().find(|case| case["name"] == "owner").unwrap();
    assert_eq!(owner["overrides"]["Maslow Settings"]["motorSpacingX"], "3011.16237");
}

#[test]
fn computing_twice_changes_nothing_more() {
    cases().iter().for_each(|case| {
        let mut settings = opened(case["overrides"].as_object().unwrap());
        recompute(&mut settings, FIRMWARE);
        assert_eq!(derived(&mut settings, |_| ()), json!({ "changes": [], "lines": [] }));
    });
}
