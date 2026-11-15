//! A Python-written webcontrol.json is written back byte for byte, and fixtures/rust_settings/rust-written.json is
//! what this writer makes of it after typical edits (the Python backend loaded that file when it was checked in).
//! `WC_REGENERATE=1 cargo test -p wc-config --test python_roundtrip` rewrites it.

use std::fs;
use std::path::PathBuf;

use serde_json::json;
use wc_config::{Paths, Settings, Source};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures/rust_settings")
}

fn opened_python_file() -> (tempfile::TempDir, Paths, Settings) {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths::new(home.path());
    fs::create_dir_all(paths.dir()).unwrap();
    fs::copy(fixtures().join("python-written.json"), paths.settings_file()).unwrap();
    let settings = Settings::open(&paths).unwrap();
    (home, paths, settings)
}

#[test]
fn a_python_written_file_is_written_back_byte_for_byte() {
    let (_home, paths, settings) = opened_python_file();
    assert_eq!(settings.source(), Source::File);
    let python = fs::read_to_string(fixtures().join("python-written.json")).unwrap();
    assert_eq!(settings.to_python_json(), python);
    assert!(!paths.settings_backup().exists(), "a complete file is not rewritten on open");
}

#[test]
fn the_rust_written_fixture_is_what_this_writer_makes() {
    let (_home, paths, mut settings) = opened_python_file();
    let edits = [
        ("Maslow Settings", "motorSpacingX", json!("3000")),
        ("Maslow Settings", "zAxis", json!(true)),
        ("Maslow Settings", "macro2_title", json!("Vide \u{2013} poussière")),
        ("WebControl Settings", "webPort", json!("5001")),
        ("WebControl Settings", "loggingTimeout", json!("2.5")),
        ("Advanced Settings", "chainOverSprocket", json!("Top")),
        ("Advanced Settings", "enablePosPIDValues", json!(true)),
        ("Advanced Settings", "KpPos", json!("1400")),
        ("Maslow Settings", "sledWeight", json!("101.25")),
    ];
    edits.iter().for_each(|(section, key, value)| {
        settings.store(section, key, value).unwrap();
        settings.compute();
    });
    settings.set_value("Advanced Settings", "maxZRPM", json!("12.6"));
    settings.save(&paths).unwrap();
    let written = fs::read_to_string(paths.settings_file()).unwrap();
    let fixture = fixtures().join("rust-written.json");
    if std::env::var_os("WC_REGENERATE").is_some() {
        fs::write(&fixture, &written).unwrap();
    }
    assert_eq!(written, fs::read_to_string(fixture).unwrap(), "rerun with WC_REGENERATE=1 if the change is meant");
}
