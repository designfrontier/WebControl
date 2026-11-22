//! The tool library: checking a tool, the library edits, and `tools.json` on disk.

use std::fs;

use serde_json::json;
use wc_config::Paths;
use wc_config::tools::{self, Refusal};
use wc_protocol::tools::{Tool, ToolLibrary, ToolUnits};

fn tool(number: u32, description: &str) -> Tool {
    Tool { number, description: description.into(), diameter: None, units: None, flutes: None, notes: String::new() }
}

fn invalid(body: serde_json::Value) -> String {
    match tools::parse_tool(&body) {
        Err(Refusal::Invalid(why)) => why,
        other => panic!("{body} gave {other:?}"),
    }
}

#[test]
fn a_full_tool_parses_trimmed() {
    let body = json!({"number": 1, "description": " 1/4\" 2 flute endmill ", "diameter": 6.35, "units": "mm",
        "flutes": 2, "notes": " upcut "});
    let parsed = tools::parse_tool(&body).unwrap();
    assert_eq!(
        parsed,
        Tool {
            number: 1,
            description: "1/4\" 2 flute endmill".into(),
            diameter: Some(6.35),
            units: Some(ToolUnits::Mm),
            flutes: Some(2),
            notes: "upcut".into(),
        }
    );
    let bare = tools::parse_tool(&json!({"number": 3, "description": "v-bit", "diameter": null, "units": "in"}));
    assert_eq!(bare.unwrap(), tool(3, "v-bit"), "units without a diameter are dropped");
}

#[test]
fn a_bad_tool_says_which_field() {
    assert!(invalid(json!([])).contains("JSON object"));
    assert!(invalid(json!({"description": "x"})).contains("number"));
    assert!(invalid(json!({"number": 0, "description": "x"})).contains("number"));
    assert!(invalid(json!({"number": 100_000, "description": "x"})).contains("number"));
    assert!(invalid(json!({"number": 1.5, "description": "x"})).contains("number"));
    assert!(invalid(json!({"number": 1, "description": "  "})).contains("Describe"));
    assert!(invalid(json!({"number": 1})).contains("Describe"));
    assert!(invalid(json!({"number": 1, "description": 7})).contains("description"));
    assert!(invalid(json!({"number": 1, "description": "x".repeat(201)})).contains("200"));
    assert!(invalid(json!({"number": 1, "description": "x", "diameter": 0, "units": "mm"})).contains("diameter"));
    assert!(invalid(json!({"number": 1, "description": "x", "diameter": "6", "units": "mm"})).contains("diameter"));
    assert!(invalid(json!({"number": 1, "description": "x", "diameter": 6})).contains("units"));
    assert!(invalid(json!({"number": 1, "description": "x", "diameter": 6, "units": "cm"})).contains("mm or in"));
    assert!(invalid(json!({"number": 1, "description": "x", "flutes": 21})).contains("Flutes"));
    assert!(invalid(json!({"number": 1, "description": "x", "notes": "x".repeat(1001)})).contains("Notes"));
}

#[test]
fn edits_keep_numbers_unique_and_sorted() {
    let library = tools::add(&ToolLibrary::default(), tool(5, "five")).unwrap();
    let library = tools::add(&library, tool(2, "two")).unwrap();
    assert_eq!(library.tools.iter().map(|t| t.number).collect::<Vec<_>>(), [2, 5]);
    assert_eq!(tools::add(&library, tool(2, "again")), Err(Refusal::Taken(2)));

    let renumbered = tools::replace(&library, 5, tool(1, "one")).unwrap();
    assert_eq!(renumbered.tools, [tool(1, "one"), tool(2, "two")]);
    assert_eq!(tools::replace(&library, 5, tool(2, "x")), Err(Refusal::Taken(2)));
    assert_eq!(tools::replace(&library, 9, tool(9, "x")), Err(Refusal::Missing(9)));
    assert_eq!(tools::replace(&library, 2, tool(2, "TWO")).unwrap().tools[0].description, "TWO");

    assert_eq!(tools::remove(&library, 2).unwrap().tools, [tool(5, "five")]);
    assert_eq!(tools::remove(&library, 3), Err(Refusal::Missing(3)));
    assert_eq!(Refusal::Taken(2).to_string(), "Your tool library already has a tool 2.");
    assert_eq!(Refusal::Missing(3).to_string(), "Your tool library has no tool 3.");
    assert_eq!(Refusal::Invalid("why".into()).to_string(), "why");
}

#[test]
fn the_library_round_trips_through_tools_json_with_a_backup() {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths::new(home.path());
    assert_eq!(tools::load(&paths).unwrap(), ToolLibrary::default());
    let first = ToolLibrary { tools: vec![tool(1, "one")] };
    tools::save(&paths, &first).unwrap();
    let second = tools::add(&first, tool(2, "two")).unwrap();
    tools::save(&paths, &second).unwrap();
    assert_eq!(tools::load(&paths).unwrap(), second);
    let backup: ToolLibrary =
        serde_json::from_str(&fs::read_to_string(paths.dir().join("tools.json.bak")).unwrap()).unwrap();
    assert_eq!(backup, first);
    assert!(!paths.dir().join("tools.json.tmp").exists());
}

#[test]
fn a_hand_edited_file_is_sorted_and_notes_may_be_missing() {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths::new(home.path());
    fs::create_dir_all(paths.dir()).unwrap();
    let text = r#"{"tools": [{"number": 4, "description": "four", "diameter": null, "units": null, "flutes": null},
        {"number": 2, "description": "two", "diameter": 3.175, "units": "mm", "flutes": 2, "notes": ""}]}"#;
    fs::write(paths.tools_file(), text).unwrap();
    let library = tools::load(&paths).unwrap();
    assert_eq!(library.tools.iter().map(|t| t.number).collect::<Vec<_>>(), [2, 4]);
}

#[test]
fn an_unreadable_file_is_set_aside_and_read_as_empty() {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths::new(home.path());
    fs::create_dir_all(paths.dir()).unwrap();
    fs::write(paths.tools_file(), "{not json").unwrap();
    assert_eq!(tools::load(&paths).unwrap(), ToolLibrary::default());
    assert_eq!(fs::read_to_string(paths.dir().join("tools.json.corrupt")).unwrap(), "{not json");
    fs::create_dir_all(paths.tools_file()).unwrap();
    assert!(tools::load(&paths).is_err(), "a directory in its place cannot be read");
}
