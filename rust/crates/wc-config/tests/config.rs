use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use wc_config::{Paths, Settings, Source, python_float_str, python_str};

fn home_with(settings: Option<&str>, backup: Option<&str>) -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    let dir = home.path().join(".WebControl");
    fs::create_dir_all(&dir).unwrap();
    settings.into_iter().for_each(|text| fs::write(dir.join("webcontrol.json"), text).unwrap());
    backup.into_iter().for_each(|text| fs::write(dir.join("webcontrol.json.bak"), text).unwrap());
    home
}

fn tree(path: &Path) -> Vec<String> {
    let mut entries: Vec<String> =
        walk(path).into_iter().map(|p| p.strip_prefix(path).unwrap().display().to_string()).collect();
    entries.sort();
    entries
}

fn walk(path: &Path) -> Vec<std::path::PathBuf> {
    fs::read_dir(path)
        .map(|dir| {
            dir.flat_map(|e| {
                let p = e.unwrap().path();
                let mut v = walk(&p);
                v.push(p);
                v
            })
            .collect()
        })
        .unwrap_or_default()
}

#[test]
fn the_layout_is_pythons() {
    let paths = Paths::new(Path::new("/home/pi"));
    assert_eq!(paths.dir(), Path::new("/home/pi/.WebControl"));
    assert_eq!(paths.settings_file(), Path::new("/home/pi/.WebControl/webcontrol.json"));
    assert_eq!(paths.gcode_dir(), Path::new("/home/pi/.WebControl/gcode"));
    assert_eq!(paths.boards_dir(), Path::new("/home/pi/.WebControl/boards"));
    assert_eq!(paths.imports_dir(), Path::new("/home/pi/.WebControl/imports"));
    assert_eq!(paths.downloads_dir(), Path::new("/home/pi/.WebControl/downloads"));
}

#[test]
fn reads_the_settings_file() {
    let file = json!({"Computed Settings": [{"key": "units", "value": "MM", "type": "string"}]});
    let home = home_with(Some(&file.to_string()), None);
    let settings = Settings::load(&Paths::new(home.path()));
    assert_eq!(settings.source(), Source::File);
    assert_eq!(settings.value("Computed Settings", "units"), Some(&json!("MM")));
}

#[test]
fn settings_the_file_lacks_come_from_the_defaults() {
    let file = json!({"Computed Settings": [{"key": "units", "value": "MM"}], "Mine": [{"key": "x", "value": 1}]});
    let home = home_with(Some(&file.to_string()), None);
    let settings = Settings::load(&Paths::new(home.path()));
    assert_eq!(settings.value("Computed Settings", "units"), Some(&json!("MM")));
    assert_eq!(settings.value("Computed Settings", "unitsZ"), Some(&json!("INCHES")));
    assert_eq!(settings.value("Maslow Settings", "macro1_title"), Some(&json!("Macro 1")));
    assert_eq!(settings.value("Mine", "x"), Some(&json!(1)));
}

#[test]
fn an_added_default_keeps_only_the_fields_python_copies() {
    let home = home_with(Some("{}"), None);
    let settings = Settings::load(&Paths::new(home.path()));
    let option = settings.option("Maslow Settings", "bedWidth").expect("bedWidth");
    let allowed = ["default", "desc", "firmwareKey", "key", "section", "title", "type", "value", "options"];
    assert!(option.keys().all(|k| allowed.contains(&k.as_str())), "{option:?}");
}

#[test]
fn a_corrupt_file_falls_back_to_the_backup_then_the_defaults() {
    let backup = json!({"Computed Settings": [{"key": "units", "value": "MM"}]}).to_string();
    let home = home_with(Some("{not json"), Some(&backup));
    let settings = Settings::load(&Paths::new(home.path()));
    assert_eq!(settings.source(), Source::Backup);
    assert_eq!(settings.value("Computed Settings", "units"), Some(&json!("MM")));

    let home = home_with(Some("[1, 2]"), Some("also not json"));
    let settings = Settings::load(&Paths::new(home.path()));
    assert_eq!(settings.source(), Source::Defaults);
    assert_eq!(settings.value("Computed Settings", "units"), Some(&json!("INCHES")));
}

#[test]
fn loading_never_writes_to_the_home() {
    let empty = tempfile::tempdir().unwrap();
    let settings = Settings::load(&Paths::new(empty.path()));
    assert_eq!(settings.source(), Source::Defaults);
    assert!(tree(empty.path()).is_empty(), "{:?}", tree(empty.path()));

    let home = home_with(Some("{\"Mine\": []}"), None);
    let before = fs::read(home.path().join(".WebControl/webcontrol.json")).unwrap();
    Settings::load(&Paths::new(home.path()));
    assert_eq!(fs::read(home.path().join(".WebControl/webcontrol.json")).unwrap(), before);
    assert_eq!(tree(home.path()), [".WebControl", ".WebControl/webcontrol.json"]);
}

#[test]
fn numbers_read_like_pythons_float() {
    let file = json!({"S": [
        {"key": "text", "value": " 2.5 "}, {"key": "int", "value": 3}, {"key": "bad", "value": "abc"},
        {"key": "flag", "value": true}, {"key": "null", "value": null}, {"key": "nan", "value": "nan"}
    ]});
    let home = home_with(Some(&file.to_string()), None);
    let settings = Settings::load(&Paths::new(home.path()));
    assert_eq!(settings.number("S", "text"), 2.5);
    assert_eq!(settings.number("S", "int"), 3.0);
    assert_eq!(settings.number("S", "bad"), 0.0);
    assert_eq!(settings.number("S", "flag"), 1.0);
    assert_eq!(settings.number("S", "null"), 0.0);
    assert_eq!(settings.number("S", "missing"), 0.0);
    assert_eq!(settings.number("S", "nan"), 0.0);
}

#[test]
fn text_is_pythons_str_and_falsy_values_are_empty() {
    let file = json!({"S": [{"key": "t", "value": "Spindle"}, {"key": "n", "value": 1.0},
        {"key": "zero", "value": 0}, {"key": "empty", "value": ""}, {"key": "no", "value": false}]});
    let home = home_with(Some(&file.to_string()), None);
    let settings = Settings::load(&Paths::new(home.path()));
    assert_eq!(settings.text_or_empty("S", "t"), "Spindle");
    assert_eq!(settings.text_or_empty("S", "n"), "1.0");
    assert_eq!(settings.text_or_empty("S", "zero"), "");
    assert_eq!(settings.text_or_empty("S", "empty"), "");
    assert_eq!(settings.text_or_empty("S", "no"), "");
    assert_eq!(settings.text_or_empty("S", "missing"), "");
    assert_eq!(settings.text("S", "t").as_deref(), Some("Spindle"));
    assert_eq!(settings.text("S", "missing"), None);
}

#[test]
fn the_web_port_comes_from_webcontrol_settings() {
    assert_eq!(Settings::defaults().web_port(), 5000);
    let file = json!({"WebControl Settings": [{"key": "webPort", "value": "5099"}]});
    let home = home_with(Some(&file.to_string()), None);
    assert_eq!(Settings::load(&Paths::new(home.path())).web_port(), 5099);
    let bad = json!({"WebControl Settings": [{"key": "webPort", "value": 99999}]});
    let home = home_with(Some(&bad.to_string()), None);
    assert_eq!(Settings::load(&Paths::new(home.path())).web_port(), 5000);
}

#[test]
fn python_str_prints_numbers_like_python() {
    let cases: [(Value, &str); 14] = [
        (json!(1.0), "1.0"),
        (json!(0.1), "0.1"),
        (json!(1e16), "1e+16"),
        (json!(1e-5), "1e-05"),
        (json!(123456789012345.0), "123456789012345.0"),
        (json!(1.5e300), "1.5e+300"),
        (json!(-0.0), "-0.0"),
        (json!(0.0001), "0.0001"),
        (json!(12345678901234567.0), "1.2345678901234568e+16"),
        (json!(7), "7"),
        (json!("x"), "x"),
        (json!(true), "True"),
        (json!(null), "None"),
        (json!([1, "a"]), "[1,\"a\"]"),
    ];
    cases.iter().for_each(|(value, text)| assert_eq!(python_str(value), *text, "{value}"));
}

#[test]
fn python_float_str_prints_what_python_prints_for_any_float() {
    let cases =
        [(f64::NAN, "nan"), (f64::INFINITY, "inf"), (f64::NEG_INFINITY, "-inf"), (470.0, "470.0"), (-0.2, "-0.2")];
    cases.iter().for_each(|(value, text)| assert_eq!(python_float_str(*value), *text));
}

#[test]
fn a_section_that_is_not_a_list_gets_the_defaults() {
    let home = home_with(Some(r#"{"Computed Settings": "garbage"}"#), None);
    let settings = Settings::load(&Paths::new(home.path()));
    assert_eq!(settings.value("Computed Settings", "units"), Some(&json!("INCHES")));
}
