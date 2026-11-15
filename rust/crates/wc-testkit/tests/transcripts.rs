use serde_json::Value;
use wc_testkit::transcript::{Event, Gcode, Start, Transcript, serial_dir};

fn schema_validator() -> jsonschema::Validator {
    let text = std::fs::read_to_string(serial_dir().join("transcript.schema.json")).unwrap();
    jsonschema::draft202012::new(&serde_json::from_str::<Value>(&text).unwrap()).unwrap()
}

#[test]
fn every_transcript_fits_the_schema_and_the_serde_types() {
    let validator = schema_validator();
    let corpus = Transcript::corpus();
    assert!(corpus.len() >= 30, "only {} transcripts", corpus.len());
    corpus.iter().for_each(|(path, transcript)| {
        let raw: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let errors: Vec<String> = validator.iter_errors(&raw).map(|e| e.to_string()).collect();
        assert!(errors.is_empty(), "{}: {errors:?}", path.display());
        assert_eq!(path.file_stem().unwrap().to_str().unwrap(), transcript.meta.name);
        assert_eq!(transcript.format_version, 1);
    });
}

#[test]
fn rust_overrides_fit_the_schema_and_replace_shared_transcripts_by_name() {
    let validator = schema_validator();
    let overrides = Transcript::overrides();
    assert!(!overrides.is_empty());
    overrides.iter().for_each(|(path, transcript)| {
        let text = std::fs::read_to_string(path).unwrap();
        let raw: Value = serde_json::from_str(&text).unwrap();
        assert!(validator.iter_errors(&raw).next().is_none(), "{}", path.display());
        assert_eq!(path.file_stem().unwrap().to_str().unwrap(), transcript.meta.name);
        assert!(transcript.meta.description.starts_with("Rust"), "{} says why Rust differs", transcript.meta.name);
    });
    let shared = Transcript::corpus();
    let rust_only: Vec<&str> = overrides
        .iter()
        .map(|(_, t)| t)
        .filter(|t| !shared.iter().any(|(_, s)| s.meta.name == t.meta.name))
        .inspect(|t| assert!(t.meta.description.starts_with("Rust only"), "{} replaces nothing", t.meta.name))
        .map(|t| t.meta.name.as_str())
        .collect();
    assert!(rust_only.contains(&"touch_z_without_zeroing_alerts"), "{rust_only:?}");
    let rust = Transcript::rust_corpus();
    assert_eq!(rust.len(), shared.len() + rust_only.len(), "each override replaces one shared transcript or none");
    let names: Vec<&str> = rust.iter().map(|(_, t)| t.meta.name.as_str()).collect();
    assert!(names.windows(2).all(|pair| pair[0] < pair[1]), "sorted and unique");
    let replaced = rust.iter().find(|(_, t)| t.meta.name == "connect_boot_handshake").unwrap();
    assert!(replaced.0.to_string_lossy().contains("transcripts-rust"));
}

#[test]
fn realtime_is_set_exactly_on_the_realtime_writes() {
    let realtime_writes = ["!\n", "~\n"];
    Transcript::corpus().iter().flat_map(|(_, t)| t.events.clone()).for_each(|event| {
        if let Event::Tx { bytes, realtime, .. } = event {
            assert_eq!(realtime, realtime_writes.contains(&bytes.as_str()), "{bytes:?}");
        }
    });
}

#[test]
fn gcode_files_named_by_path_exist() {
    let with_paths: Vec<Transcript> = Transcript::corpus()
        .into_iter()
        .map(|(_, t)| t)
        .filter(|t| matches!(t.setup.gcode, Some(Gcode::Path { .. })))
        .collect();
    assert!(!with_paths.is_empty());
    with_paths.iter().for_each(|t| assert!(!t.gcode_lines().unwrap().is_empty(), "{}", t.meta.name));
}

#[test]
fn inline_gcode_and_missing_gcode_load_as_given() {
    let inline = r#"{"formatVersion":1,"meta":{"name":"x","description":"d","firmware":{"variant":"holey","version":"51.27"},
        "webcontrolCommit":"c"},"setup":{"gcode":{"lines":["G0 X1","G1 Y2"]}},"events":[]}"#;
    let transcript = Transcript::parse(inline).unwrap();
    assert_eq!(transcript.gcode_lines().unwrap(), ["G0 X1", "G1 Y2"]);
    assert_eq!(transcript.setup.start, Start::Connected);
    assert_eq!(transcript.timing.quiet, 0.1);
    let none = inline.replace(r#""gcode":{"lines":["G0 X1","G1 Y2"]}"#, "");
    assert_eq!(Transcript::parse(&none).unwrap().gcode_lines(), None);
}

#[test]
fn unknown_fields_are_refused() {
    let typo = r#"{"formatVersion":1,"meta":{"name":"x","description":"d","firmware":{"variant":"holey","version":"51.27"},
        "webcontrolCommit":"c"},"setup":{"unit":"MM"},"events":[]}"#;
    assert!(Transcript::parse(typo).unwrap_err().contains("unit"));
    let bad_event =
        typo.replace(r#""unit":"MM""#, "").replace(r#""events":[]"#, r#""events":[{"type":"rx","lin":"ok"}]"#);
    assert!(Transcript::parse(&bad_event).is_err());
    assert!(Transcript::load(std::path::Path::new("/nonexistent.json")).unwrap_err().contains("nonexistent"));
}
