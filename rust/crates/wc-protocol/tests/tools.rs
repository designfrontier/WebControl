//! The tool library types, each checked against its closed schema.

use serde_json::json;
use wc_protocol::main::ActivateModal;
use wc_protocol::tools::{FileTool, FileTools, Tool, ToolLibrary, ToolUnits};
use wc_protocol::{Envelope, Message};
use wc_testkit::contract::Contract;

fn library() -> ToolLibrary {
    let tool = Tool {
        number: 1,
        description: "1/4\" 2 flute endmill".into(),
        diameter: Some(6.35),
        units: Some(ToolUnits::Mm),
        flutes: Some(2),
        notes: String::new(),
    };
    ToolLibrary { tools: vec![tool] }
}

#[test]
fn the_library_fits_every_answer_and_its_broadcast() {
    let value = serde_json::to_value(library()).unwrap();
    ["get-api-tools.200", "post-api-tools.201", "put-api-tools-number.200", "delete-api-tools-number.200"]
        .iter()
        .for_each(|schema| Contract::shared().check(&format!("schemas/http/{schema}.json"), &value).unwrap());
    assert_eq!(value["tools"][0]["units"], "mm");
    let envelope = serde_json::to_value(Envelope::json(&library())).unwrap();
    Contract::shared().validate_envelope(&envelope).unwrap();
    assert_eq!(ToolLibrary::COMMAND, "toolLibrary");
}

#[test]
fn a_stored_tool_without_notes_reads_as_empty_notes() {
    let tool: Tool = serde_json::from_value(
        json!({"number": 2, "description": "x", "diameter": 0.25, "units": "in", "flutes": null}),
    )
    .unwrap();
    assert_eq!((tool.units, tool.notes), (Some(ToolUnits::In), String::new()));
}

#[test]
fn the_files_tools_fit_their_schema() {
    let tools = FileTools {
        tools: vec![
            FileTool {
                number: 6,
                description: Some("flat end mill".into()),
                diameter: Some(6.35),
                units: Some(ToolUnits::Mm),
                flutes: None,
            },
            FileTool { number: 10, description: None, diameter: None, units: None, flutes: None },
        ],
    };
    let value = serde_json::to_value(tools).unwrap();
    Contract::shared().check("schemas/http/get-api-files-gcode-tools.200.json", &value).unwrap();
}

#[test]
fn a_tool_change_notification_names_its_tool() {
    let prompt = serde_json::to_value(ActivateModal::tool_change("Please insert tool 3", Some(3))).unwrap();
    Contract::shared().validate_message("activateModal", &prompt).unwrap();
    assert_eq!((prompt["tool"].clone(), prompt["resume"].clone()), (json!(3), json!("resume")));
    let alert = serde_json::to_value(ActivateModal::alert("x")).unwrap();
    assert!(alert.get("tool").is_none());
}
