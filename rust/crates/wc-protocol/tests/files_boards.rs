//! The files, boards and viewer types, each checked against its closed schema.

use serde_json::{Value, json};
use wc_protocol::boards::{BoardDataUpdate, BoardUnits, CurrentBoard, Router};
use wc_protocol::files::{
    ApiError, ClassicResult, ClassicStatus, CreatedDirectory, DirectoryRequest, GcodeDelete, GcodeText, Library,
    LibraryFile, LibraryListing, LoadedGcode, UpdateDirectories,
};
use wc_protocol::main::GcodePositionMessage;
use wc_protocol::{ClientEvent, Envelope, Frame};
use wc_testkit::contract::Contract;

fn check(schema: &str, value: impl serde::Serialize) -> Value {
    let value = serde_json::to_value(value).unwrap();
    Contract::shared().check(&format!("schemas/http/{schema}.json"), &value).unwrap();
    value
}

fn envelope(envelope: Envelope) {
    Contract::shared().validate_envelope(&serde_json::to_value(envelope).unwrap()).unwrap();
}

fn files() -> Vec<LibraryFile> {
    vec![LibraryFile { directory: ".".into(), name: "part.nc".into() }]
}

#[test]
fn library_listings_fit_their_schemas() {
    let gcode = Library {
        directories: vec![".".into()],
        files: files(),
        last_directory: ".".into(),
        last_file: "part.nc".into(),
        valid_extensions: Some(".nc".into()),
    };
    assert_eq!(check("get-api-files-gcode.200", &gcode)["validExtensions"], ".nc");
    let boards = Library { valid_extensions: None, ..gcode };
    assert!(check("get-api-boards-files.200", &boards).get("validExtensions").is_none());
    let created = CreatedDirectory { directory: "jobs".into(), directories: vec![".".into()], files: files() };
    check("post-api-files-gcode-directories.201", &created);
    check("post-api-boards-directories.201", &created);
    check("post-api-files-gcode-directories.400", ApiError { error: "nope".into() });
}

#[test]
fn a_delete_takes_one_path_and_answers_the_listing() {
    check("post-api-files-gcode-delete.200", LibraryListing { directories: vec![".".into()], files: files() });
    check("post-api-files-gcode-delete.409", ApiError { error: "In use".into() });
    let delete: GcodeDelete = serde_json::from_value(json!({"path": "jobs/leg.nc"})).unwrap();
    assert_eq!(delete.path, "jobs/leg.nc");
    assert!(serde_json::from_value::<GcodeDelete>(json!({"path": "jobs", "x": 1})).is_err());
}

#[test]
fn the_loaded_gcode_fits_its_schemas() {
    check("get-api-files-gcode-loaded.200", LoadedGcode { directory: None, name: None, line_count: 0 });
    let loaded = LoadedGcode { directory: Some("jobs".into()), name: Some("leg.nc".into()), line_count: 3 };
    assert_eq!(
        check("get-api-files-gcode-loaded.200", loaded),
        json!({"directory": "jobs", "name": "leg.nc",
        "lineCount": 3})
    );
    check("get-api-files-gcode-text.200", GcodeText { text: "G0 X0".into() });
    check("get-api-files-gcode-custom.200", GcodeText { text: String::new() });
}

#[test]
fn classic_answers_repeat_the_status() {
    assert_eq!(check("post-opengcode.200", ClassicStatus::new(200)), json!({"status": 200}));
    check("post-opengcode.500", ClassicStatus::new(500));
    let refused = check("post-opengcode.409", ClassicStatus::refused(409, "Stop the job first."));
    assert_eq!(refused, json!({"status": 409, "error": "Stop the job first."}));
    check("post-opengcode.400", ClassicStatus::refused(400, "Path is outside the library"));
    assert_eq!(check("post-editgcode.200", ClassicResult::Success), json!("success"));
    assert_eq!(check("post-editgcode.500", ClassicResult::Failed), json!("failed"));
}

#[test]
fn the_current_board_fits_its_schema() {
    let board = CurrentBoard {
        board_id: "Shelf".into(),
        material: "Plywood".into(),
        width: 48.0,
        height: 24.0,
        thickness: 0.75,
        center_x: 1.5,
        center_y: -2.0,
        units: BoardUnits::Inches,
        file_name: "shelf.brd".into(),
        router: Router { x: 3.0, y: -4.5 },
    };
    let value = check("get-api-boards-current.200", board);
    assert_eq!((value["boardID"].clone(), value["units"].clone()), (json!("Shelf"), json!("inches")));
    assert_eq!(serde_json::to_value(BoardUnits::Mm).unwrap(), "mm");
}

#[test]
fn files_boards_and_viewer_messages_fit_their_schemas() {
    envelope(Envelope::json(&UpdateDirectories { directory: "jobs".into() }));
    envelope(Envelope::json(&GcodePositionMessage {
        xval: 1.0,
        yval: 2.0,
        zval: -1.0,
        gcode_line: "G1 X1 ".into(),
        gcode_line_index: 3,
    }));
    let info = json!({"width": "12", "height": 24.0, "thickness": 0.75, "centerX": 0, "centerY": 0,
        "boardID": "-", "material": "-", "fileName": ""});
    envelope(Envelope::json(&BoardDataUpdate(info)));
    envelope(Envelope::int("showFPSpinner", 42));
    envelope(Envelope::base64("gcodeUpdateCompressed", None));
    envelope(Envelope::base64("boardCutDataUpdateCompressed", Some(b"\x1f\x8b")));
}

#[test]
fn a_directory_request_names_what_python_would_str() {
    let name = |body: &[u8]| DirectoryRequest::from_body(body).name;
    assert_eq!(name(br#"{"name": " jobs "}"#), " jobs ");
    assert_eq!(name(br#"{"name": 5}"#), "5");
    assert_eq!(name(br#"{"name": true}"#), "True");
    assert_eq!(name(br#"{"name": false}"#), "False");
    assert_eq!(name(br#"{"name": null}"#), "None");
    assert_eq!(name(br#"{}"#), "");
    assert_eq!(name(b"nope"), "");
    Contract::shared().validate_request("POST", "/api/files/gcode/directories", &json!({"name": "jobs"})).unwrap();
}

#[test]
fn the_update_requests_decode() {
    let frame = |event: &str| Frame { ns: "/MaslowCNC".into(), event: event.into(), data: json!({"data": "Please"}) };
    Contract::shared().validate_frame(&frame("checkForGCodeUpdate").encode()).unwrap();
    assert_eq!(ClientEvent::from_frame(&frame("checkForGCodeUpdate")), Some(ClientEvent::CheckForGcodeUpdate));
    assert_eq!(ClientEvent::from_frame(&frame("checkForBoardUpdate")), Some(ClientEvent::CheckForBoardUpdate));
}
