//! The loaded gcode and the board: loading, the preview pushes, moving through the file, the board actions and the
//! requests the file routes make (ported from test_api_files, test_gcode_load_guard, test_restore and the board
//! half of test_actions_characterization).

mod rig;

use std::io::Read;
use std::path::PathBuf;

use flate2::read::GzDecoder;
use rig::{Rig, alerts, commands, pushed, settings};
use serde_json::{Value, json};
use wc_config::Settings;
use wc_gcode::estimate::MOVE_S;
use wc_machine::files::{BoardForm, GCODE_LOCKED, Gcode, Origin, Problem, Reply, Request, TrimForm};
use wc_machine::{Effect, Event};
use wc_protocol::main::Units;

const SQUARE: &str = "G21\nG90\nG0 X0 Y0 Z5\nG1 Z-1 F500\nG1 X50 Y0\nG1 X50 Y50\nG1 X0 Y50\nG1 X0 Y0\nG0 Z5\n";

fn parse(text: &str) -> Gcode {
    Gcode::parse(text, &settings(&[])).unwrap()
}

/// Sends a request and returns its reply with the other effects.
fn ask(rig: &mut Rig, request: Request) -> (Reply, Vec<Effect>) {
    let effects = rig.send(Event::Request { id: 7, request });
    let reply = effects.iter().find_map(|effect| match effect {
        Effect::Reply { id: 7, reply } => Some(reply.clone()),
        _ => None,
    });
    (reply.expect("every request is answered"), effects)
}

fn open(rig: &mut Rig, text: &str, file: &str) -> (Reply, Vec<Effect>) {
    let request = Request::LoadGcode { gcode: Box::new(parse(text)), file: Some(file.into()), origin: Origin::Open };
    ask(rig, request)
}

fn inflate(base64: &Value) -> Value {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(base64.as_str().unwrap()).unwrap();
    let mut text = String::new();
    GzDecoder::new(&bytes[..]).read_to_string(&mut text).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn saved(effects: &[Effect]) -> bool {
    effects.iter().any(|effect| matches!(effect, Effect::SaveSettings(_)))
}

/// A machine past its first status push.
fn quiet() -> Rig {
    let mut rig = Rig::new();
    rig.send(Event::Tick);
    rig
}

fn streaming() -> Rig {
    let mut rig = Rig::connected();
    rig.machine.load_gcode("G0 X1\nG0 X2\nG0 X3\n");
    rig.act("startRun");
    rig
}

#[test]
fn opening_a_file_starts_at_line_1_and_reports_its_position() {
    let mut rig = Rig::new();
    let (reply, effects) = open(&mut rig, SQUARE, "/home/pi/.WebControl/gcode/square.nc");
    assert_eq!(reply, Reply::Done);
    assert_eq!(
        pushed(&effects, "gcodePositionMessage"),
        [json!({"xval": 0.0, "yval": 0.0, "zval": 0.0, "gcodeLine": "G21 ", "gcodeLineIndex": 0})]
    );
    assert!(pushed(&effects, "gcodeUpdateCompressed").is_empty(), "the client asks for the preview");
    let snapshot = rig.machine.snapshot();
    assert_eq!(snapshot.gcode_count, 10);
    assert_eq!(snapshot.gcode_file, Some(PathBuf::from("/home/pi/.WebControl/gcode/square.nc")));
    assert_eq!(snapshot.gcode[3], "G1 Z-1 F500 ");
    assert!(!snapshot.gcode_locked);
}

#[test]
fn a_files_units_set_the_tolerance_setting() {
    let mut rig = Rig::new();
    let (_, effects) = open(&mut rig, "G20\nG0 Z1\n", "a.nc");
    assert_eq!(rig.machine.snapshot().settings.value("Computed Settings", "tolerance"), Some(&json!(0.02)));
    assert!(!saved(&effects), "0.02 is already stored");
    let (_, effects) = open(&mut rig, SQUARE, "b.nc");
    assert_eq!(rig.machine.snapshot().settings.value("Computed Settings", "tolerance"), Some(&json!(0.5)));
    assert!(saved(&effects));
}

#[test]
fn the_preview_is_pushed_on_request_with_its_size_first() {
    let mut rig = quiet();
    let nothing = rig.send(Event::CheckForGcodeUpdate);
    assert_eq!(commands(&nothing), ["requestedSetting", "gcodeUpdateCompressed"]);
    assert_eq!(pushed(&nothing, "gcodeUpdateCompressed"), [json!("")]);
    open(&mut rig, SQUARE, "square.nc");
    let effects = rig.send(Event::CheckForGcodeUpdate);
    assert_eq!(commands(&effects), ["requestedSetting", "showFPSpinner", "gcodeUpdateCompressed"]);
    let size = pushed(&effects, "showFPSpinner")[0].as_u64().unwrap();
    let preview = pushed(&effects, "gcodeUpdateCompressed")[0].clone();
    assert_eq!(size as usize, rig.machine.snapshot().preview.compressed.as_ref().unwrap().len());
    let moves = inflate(&preview);
    assert!(moves.as_array().unwrap().iter().any(|m| m["type"] == "line" && m["dashed"] == false));
}

#[test]
fn a_file_with_arc_centre_modes_gets_a_console_note_that_they_are_converted() {
    let mut rig = Rig::new();
    let (_, effects) = open(&mut rig, "G00G21G17G90G40G49G80\nG71G91.1\nG0 X1\n", "vcarve.nc");
    let note = "This file uses G91.1; WebControl converts it for the Maslow firmware.";
    assert_eq!(pushed(&effects, "controllerMessage"), [json!(note)]);
    let (_, effects) = open(&mut rig, "G21\nG90.1\nG2 X1 I1 J0\n", "absolute.nc");
    assert_eq!(pushed(&effects, "controllerMessage").len(), 1);
    let (_, plain) = open(&mut rig, SQUARE, "square.nc");
    assert!(pushed(&plain, "controllerMessage").is_empty());
    let edit = Request::LoadGcode { gcode: Box::new(parse("G91.1\nG0 X2")), file: None, origin: Origin::Edit };
    assert!(pushed(&ask(&mut rig, edit).1, "controllerMessage").is_empty());
}

#[test]
fn an_edit_keeps_the_index_where_it_fits_and_pushes_the_preview() {
    let mut rig = Rig::new();
    open(&mut rig, SQUARE, "square.nc");
    rig.act_with("moveGcodeGoto", json!(8), json!(""));
    let edit = Request::LoadGcode { gcode: Box::new(parse("G0 X1\nG0 X2")), file: None, origin: Origin::Edit };
    let (reply, effects) = ask(&mut rig, edit);
    assert_eq!(reply, Reply::Done);
    assert_eq!(rig.machine.snapshot().gcode_index, 1);
    assert_eq!(rig.machine.snapshot().gcode_file, Some(PathBuf::from("square.nc")), "an edit keeps the name");
    let estimate = ["gcodePositionMessage", "showFPSpinner", "gcodeUpdateCompressed", "pacingStats"];
    assert_eq!(commands(&effects), estimate, "the edit's estimate goes out with the pacing");
    let reload =
        Request::LoadGcode { gcode: Box::new(parse(SQUARE)), file: Some("again.nc".into()), origin: Origin::Reload };
    let (_, effects) = ask(&mut rig, reload);
    assert_eq!(rig.machine.snapshot().gcode_index, 0);
    assert_eq!(rig.machine.snapshot().gcode_file, Some(PathBuf::from("again.nc")));
    assert_eq!(pushed(&effects, "showFPSpinner").len(), 1);
}

/// Ported from test_gcode_load_guard.
#[test]
fn the_gcode_cannot_be_replaced_during_a_job() {
    let mut rig = streaming();
    let (reply, _) = open(&mut rig, "G0 X9", "other.nc");
    assert_eq!(reply, Reply::Refused(GCODE_LOCKED.into()));
    assert!(rig.machine.snapshot().gcode_locked);
    assert_eq!(rig.machine.snapshot().gcode[0], "G0 X1 ");
    assert_eq!(alerts(&rig.act("clearGCode")), ["Cannot issue command while sending gcode."]);
}

#[test]
fn truncate_cuts_long_decimals_when_the_setting_is_on() {
    let on = settings(&[("Advanced Settings", "truncate", json!(1)), ("Advanced Settings", "digits", json!("2"))]);
    let gcode = Gcode::parse("G1 X1.23456 Y2", &on).unwrap();
    assert_eq!((gcode.lines[0].as_str(), gcode.truncate), ("G1 X1.23 Y2 ", Some(2)));
    let number = settings(&[("Advanced Settings", "truncate", json!(true)), ("Advanced Settings", "digits", json!(4))]);
    assert!(Gcode::parse("G1 X1", &number).is_err(), "Python cannot splice a number into its pattern");
    let zero = settings(&[("Advanced Settings", "truncate", json!(1)), ("Advanced Settings", "digits", json!("0"))]);
    assert!(Gcode::parse("G1 X1", &zero).is_err());
    assert_eq!(Gcode::parse("G1 X1.23456", &settings(&[])).unwrap().truncate, None);
}

#[test]
fn clear_gcode_empties_the_file_and_pushes_an_empty_preview() {
    let mut rig = Rig::new();
    open(&mut rig, SQUARE, "square.nc");
    let effects = rig.act("clearGCode");
    assert_eq!(commands(&effects), ["showFPSpinner", "gcodeUpdateCompressed", "gcodeFile", "pacingStats"]);
    assert_eq!(pushed(&effects, "pacingStats")[0]["estimateS"], json!(null));
    assert_eq!(pushed(&effects, "gcodeFile"), [json!({"file": null})]);
    assert_eq!(inflate(&pushed(&effects, "gcodeUpdateCompressed")[0]), json!([]));
    assert_eq!(rig.machine.snapshot().gcode_count, 0);
    assert_eq!(rig.machine.snapshot().gcode_file, Some(PathBuf::from("square.nc")), "Python keeps the name");
}

fn in_library(root: &str) -> Rig {
    let mut rig = Rig::new();
    let machine = std::mem::replace(&mut rig.machine, wc_machine::Machine::new("h", settings(&[])));
    rig.machine = machine.with_library(root.into());
    rig
}

#[test]
fn the_loaded_file_is_named_by_its_path_in_the_library_and_announced_when_it_changes() {
    let mut rig = in_library("/home/pi/.WebControl/gcode");
    assert_eq!(rig.machine.snapshot().gcode_name, None);
    let (_, effects) = open(&mut rig, SQUARE, "/home/pi/.WebControl/gcode/mancala/mancala.frd.roughing.nc");
    assert_eq!(pushed(&effects, "gcodeFile"), [json!({"file": "mancala/mancala.frd.roughing.nc"})]);
    assert_eq!(rig.machine.snapshot().gcode_name.as_deref(), Some("mancala/mancala.frd.roughing.nc"));
    let (_, again) = open(&mut rig, SQUARE, "/home/pi/.WebControl/gcode/mancala/mancala.frd.roughing.nc");
    assert!(pushed(&again, "gcodeFile").is_empty(), "the same file again is no change");
    let edit = Request::LoadGcode { gcode: Box::new(parse("G0 X1")), file: None, origin: Origin::Edit };
    assert!(pushed(&ask(&mut rig, edit).1, "gcodeFile").is_empty(), "an edit keeps the name");
    let (_, outside) = open(&mut rig, SQUARE, "/tmp/elsewhere/part.nc");
    assert_eq!(pushed(&outside, "gcodeFile"), [json!({"file": "part.nc"})], "outside the library, the name alone");
    let resync = rig.send(Event::ClientResync { sid: "abc".into() });
    assert_eq!(pushed(&resync, "gcodeFile"), [json!({"file": "part.nc"})]);
    let (_, unsaved) = ask(&mut rig, Request::Problem(Problem::GcodeUnsaved));
    assert_eq!(pushed(&unsaved, "gcodeFile"), [json!({"file": null})]);
    assert_eq!(rig.machine.snapshot().gcode_name, None);
}

#[test]
fn no_gcode_or_no_file_names_nothing() {
    let mut rig = in_library("/lib");
    let gcode = Box::new(Gcode::empty(Units::Mm));
    let empty = Request::LoadGcode { gcode, file: Some("/lib/empty.nc".into()), origin: Origin::Open };
    let (_, effects) = ask(&mut rig, empty);
    assert!(pushed(&effects, "gcodeFile").is_empty());
    assert_eq!(rig.machine.snapshot().gcode_name, None);
    let mut bare = Rig::new();
    open(&mut bare, SQUARE, "/somewhere/square.nc");
    assert_eq!(bare.machine.snapshot().gcode_name.as_deref(), Some("square.nc"), "no library, the name alone");
    let effects = bare.act("clearGCode");
    assert_eq!(pushed(&effects, "gcodeFile"), [json!({"file": null})]);
    assert!(pushed(&bare.act("clearGCode"), "gcodeFile").is_empty(), "already cleared");
}

#[test]
fn the_position_message_converts_the_files_units_to_the_machines() {
    let mut rig = Rig::with(settings(&[("Computed Settings", "units", json!("INCHES"))]));
    open(&mut rig, "G21\nG0 X25.4 Y50.8 Z-2.54\nG0 X0\n", "mm.nc");
    let effects = rig.act_with("moveGcodeIndex", json!(2), json!(""));
    let position = &pushed(&effects, "gcodePositionMessage")[0];
    let near = |key: &str, expected: f64| assert!((position[key].as_f64().unwrap() - expected).abs() < 1e-12, "{key}");
    near("xval", 1.0);
    near("yval", 2.0);
    near("zval", -0.1);
    let mut metric = Rig::new();
    open(&mut metric, "G20\nG0 X1\nG0 Y1\n", "inch.nc");
    let effects = metric.act_with("moveGcodeGoto", json!("2"), json!(""));
    assert_eq!(pushed(&effects, "gcodePositionMessage")[0]["xval"], json!(25.4));
}

/// Ported from test_actions_characterization's moveGcode cases.
#[test]
fn the_index_moves_by_lines_to_a_line_or_by_z_moves() {
    let mut rig = Rig::new();
    let file = "G21\nG0 Z5\nG1 X1\nG1 Z-1\nG1 X2\nG1 Z-2\nG1 X3\nG0 Z5\n";
    open(&mut rig, file, "z.nc");
    let index = |rig: &Rig| rig.machine.snapshot().gcode_index;
    rig.act_with("moveGcodeIndex", json!(3), json!(""));
    assert_eq!(index(&rig), 3);
    rig.act_with("moveGcodeIndex", json!(-10), json!(""));
    assert_eq!(index(&rig), 0);
    rig.act_with("moveGcodeGoto", json!(100), json!(""));
    assert_eq!(index(&rig), 8, "the last line");
    rig.act_with("moveGcodeGoto", json!(0), json!(""));
    rig.act_with("moveGcodeZ", json!(1), json!(""));
    assert_eq!(index(&rig), 1);
    rig.act_with("moveGcodeZ", json!(1), json!(""));
    assert_eq!(index(&rig), 3);
    rig.act_with("moveGcodeZ", json!(2), json!(""));
    assert_eq!(index(&rig), 7);
    rig.act_with("moveGcodeZ", json!(-1), json!(""));
    assert_eq!(index(&rig), 5);
    rig.act_with("moveGcodeZ", json!(0), json!(""));
    assert_eq!(index(&rig), 5);
    rig.act_with("moveGcodeZ", json!(-3), json!(""));
    assert_eq!(index(&rig), 0, "Python's loop keeps the last match, its negative index counting from the end");
    rig.act_with("moveGcodeGoto", json!(4), json!(""));
    assert_eq!(alerts(&rig.act_with("moveGcodeZ", json!(9), json!(""))), ["Error with moving to Z move"]);
    assert_eq!(index(&rig), 4);
}

#[test]
fn moving_through_no_gcode_alerts_and_bad_arguments_do_nothing() {
    let mut rig = Rig::new();
    assert_eq!(alerts(&rig.act_with("moveGcodeIndex", json!(1), json!(""))), ["Error with moving to index"]);
    assert_eq!(alerts(&rig.act_with("moveGcodeGoto", json!(1), json!(""))), ["Error with moving to Z move"]);
    assert_eq!(alerts(&rig.act_with("moveGcodeZ", json!(1), json!(""))), ["Error with moving to Z move"]);
    open(&mut rig, SQUARE, "square.nc");
    ["1.5", "", "x"].iter().for_each(|arg| {
        assert!(commands(&rig.act_with("moveGcodeIndex", json!(arg), json!(""))).is_empty(), "{arg}");
    });
    rig.act_with("moveGcodeIndex", json!(2.7), json!(""));
    assert_eq!(rig.machine.snapshot().gcode_index, 2, "int() truncates a float");
    rig.act_with("moveGcodeIndex", json!(true), json!(""));
    assert_eq!(rig.machine.snapshot().gcode_index, 3);
    assert!(commands(&rig.act_with("moveGcodeIndex", json!(null), json!(""))).is_empty());
}

#[test]
fn a_line_whose_position_python_cannot_read_alerts() {
    let mut rig = Rig::new();
    open(&mut rig, "G0 X`1\nG0 X2\n", "bad.nc");
    assert_eq!(alerts(&rig.act_with("moveGcodeGoto", json!(1), json!(""))), ["Error with moving to Z move"]);
}

#[test]
fn define_home_resends_the_units_and_the_preview() {
    let mut rig = Rig::connected();
    let effects = rig.act_with("defineHome", json!(1), json!(1));
    assert_eq!(commands(&effects), ["homePositionMessage", "requestedSetting", "gcodeUpdateCompressed"]);
}

#[test]
fn the_board_starts_as_the_bed_and_is_pushed_on_request() {
    let mut rig = quiet();
    let effects = rig.send(Event::CheckForBoardUpdate);
    assert_eq!(commands(&effects), ["boardDataUpdate", "showFPSpinner", "boardCutDataUpdateCompressed"]);
    let info = &pushed(&effects, "boardDataUpdate")[0];
    assert_eq!((info["width"].clone(), info["height"].clone()), (json!(96.0), json!(48.0)));
    assert_eq!(info["thickness"], 0.75);
    assert_eq!(pushed(&effects, "showFPSpinner"), [json!(1)]);
    assert_eq!(inflate(&pushed(&effects, "boardCutDataUpdateCompressed")[0]), json!([]));
}

fn cut_cells(effects: &[Effect]) -> Vec<bool> {
    let cells = inflate(&pushed(effects, "boardCutDataUpdateCompressed")[0]);
    cells.as_array().unwrap().iter().map(|c| c.as_bool().unwrap()).collect()
}

#[test]
fn processing_the_gcode_marks_the_board_and_clearing_empties_it() {
    let mut rig = Rig::new();
    let edit = BoardForm {
        board_id: "Test".into(),
        material: "MDF".into(),
        width: "10".into(),
        height: "10".into(),
        thickness: "0.5".into(),
        center_x: "0".into(),
        center_y: "0".into(),
        units: "inches".into(),
    };
    assert_eq!(ask(&mut rig, Request::EditBoard(edit)).0, Reply::Done);
    open(&mut rig, SQUARE, "square.nc");
    let effects = rig.act("boardProcessGCode");
    let cells = cut_cells(&effects);
    assert_eq!(cells.len(), 100);
    assert!(cells.iter().any(|c| *c));
    let cleared = rig.act("boardClearBoard");
    assert_eq!(pushed(&cleared, "boardCutDataUpdateCompressed"), [json!("")]);
    assert!(rig.machine.snapshot().board.cut_points.is_empty());
}

#[test]
fn a_board_python_cannot_grid_is_left_alone() {
    let mut rig = Rig::new();
    rig.machine.handle_inline(
        Event::Request { id: 1, request: Request::LoadBoard(board_file("\"wide\"")) },
        Default::default(),
    );
    assert!(rig.act("boardProcessGCode").is_empty());
}

fn board_file(width: &str) -> String {
    format!(
        "{{\"width\": {width}, \"height\": 2, \"thickness\": 0.75, \"centerX\": 0, \"centerY\": 0, \"boardID\": \"B\", \
         \"material\": \"M\", \"fileName\": \"b.brd\"}}\n"
    )
}

#[test]
fn boards_load_save_edit_and_trim_through_requests() {
    let mut rig = Rig::new();
    let (reply, effects) = ask(&mut rig, Request::LoadBoard(board_file("4")));
    assert_eq!(reply, Reply::Done);
    assert_eq!(pushed(&effects, "boardDataUpdate")[0]["boardID"], "B");
    let (reply, effects) = ask(&mut rig, Request::LoadBoard("not a board".into()));
    assert_eq!((reply, alerts(&effects)), (Reply::Failed, vec!["Cannot open board.".to_string()]));
    let (reply, _) = ask(&mut rig, Request::SaveBoard("shelf.brd".into()));
    let Reply::Text(text) = reply else { panic!("{reply:?}") };
    assert!(text.starts_with("{\"width\": 4, "), "{text}");
    assert_eq!(rig.machine.snapshot().board.file_name, "shelf.brd");
    let trim =
        TrimForm { top: "0".into(), bottom: "0".into(), left: "1".into(), right: "0".into(), units: "inches".into() };
    let (reply, effects) = ask(&mut rig, Request::TrimBoard(trim.clone()));
    assert_eq!(reply, Reply::Done);
    assert_eq!(pushed(&effects, "boardDataUpdate")[0]["width"], 3.0);
    let bad = TrimForm { left: "x".into(), ..trim };
    let (reply, effects) = ask(&mut rig, Request::TrimBoard(bad));
    assert_eq!(reply, Reply::Failed);
    assert_eq!(commands(&effects)[0], "boardDataUpdate", "Python resends the board even so");
    assert_eq!(ask(&mut rig, Request::EditBoard(BoardForm::default())).0, Reply::Failed);
    rig.act("boardClearBoard");
    let (reply, effects) = ask(&mut rig, Request::SaveBoard("x.brd".into()));
    assert_eq!((reply, alerts(&effects)), (Reply::Failed, vec!["Cannot save board.".to_string()]));
}

#[test]
fn file_problems_raise_pythons_alerts() {
    let mut rig = Rig::new();
    open(&mut rig, SQUARE, "square.nc");
    let (_, effects) = ask(&mut rig, Request::Problem(Problem::GcodeUnsaved));
    assert_eq!(alerts(&effects), ["Cannot save gcode file."]);
    assert_eq!(rig.machine.snapshot().gcode_file, None);
    let (_, effects) = ask(&mut rig, Request::Problem(Problem::GcodeUnreadable));
    assert_eq!(alerts(&effects), ["Cannot open gcode file."]);
    let (_, effects) = ask(&mut rig, Request::Problem(Problem::ImportUnreadable));
    assert_eq!(pushed(&effects, "activateModal")[0]["message"], "Cannot open import file.");
    assert_eq!(pushed(&effects, "activateModal")[0]["modalType"], "notification");
}

#[test]
fn send_gcode_queues_each_line_and_remembers_the_text() {
    let mut rig = Rig::connected();
    let (reply, _) = ask(&mut rig, Request::SendGcode("G0 X5\r\nG0 Y5\n\nG0 Z1".into()));
    assert_eq!(reply, Reply::Done);
    assert_eq!(rig.run_to_quiet(), ["G0 X5", "G0 Y5", "G0 Z1"]);
    assert_eq!(rig.machine.snapshot().custom_gcode.as_deref(), Some("G0 X5\r\nG0 Y5\n\nG0 Z1"));
    rig.act("touchZ");
    let (reply, effects) = ask(&mut rig, Request::SendGcode("G0 X1".into()));
    assert_eq!(
        (reply, alerts(&effects)),
        (Reply::Failed, vec!["Wait for Touch Z to finish or press Stop".to_string()])
    );
}

#[test]
fn settings_requests_set_and_save() {
    let mut rig = Rig::new();
    let change = vec![("Maslow Settings".into(), "openFile".into(), json!("part.nc"))];
    let (reply, effects) = ask(&mut rig, Request::SetSettings(change));
    assert_eq!(reply, Reply::Done);
    assert!(saved(&effects));
    assert_eq!(rig.machine.snapshot().settings.text_or_empty("Maslow Settings", "openFile"), "part.nc");
}

/// Ported from test_restore: a restore replaces the settings and asks the controller for its own.
#[test]
fn imports_and_restores_replace_the_settings_and_ask_for_dollar_dollar() {
    let mut rig = Rig::connected();
    let mut restored = Settings::defaults();
    restored.set_value("Maslow Settings", "COMport", json!("/dev/ttyRESTORED"));
    let (reply, effects) = ask(&mut rig, Request::RestoreSettings(Box::new(restored)));
    assert_eq!(reply, Reply::Done);
    assert!(saved(&effects));
    assert_eq!(rig.machine.snapshot().settings.text_or_empty("Maslow Settings", "COMport"), "/dev/ttyRESTORED");
    assert_eq!(rig.run_to_quiet(), ["$$"]);
    let (reply, _) = ask(&mut rig, Request::ImportIni("[Maslow Settings]\nCOMport = /dev/ttyINI\n".into()));
    assert_eq!(reply, Reply::Done);
    assert_eq!(rig.machine.snapshot().settings.text_or_empty("Maslow Settings", "COMport"), "/dev/ttyINI");
    assert_eq!(rig.run_to_quiet(), ["$$"]);
    let (reply, effects) = ask(&mut rig, Request::ImportIni("COMport = x".into()));
    assert_eq!(reply, Reply::Failed);
    assert_eq!(pushed(&effects, "activateModal")[0]["message"], "Cannot open import file.");
    let imported = json!({"Maslow Settings": [{"key": "COMport", "value": "/dev/ttyJSON"}]});
    assert_eq!(ask(&mut rig, Request::ImportWebControl(imported.clone())).0, Reply::Done);
    assert_eq!(rig.run_to_quiet(), ["$$"]);
    assert_eq!(ask(&mut rig, Request::ImportWebControl(imported)).0, Reply::Done);
    assert!(rig.run_to_quiet().is_empty(), "nothing changed, so nothing to compare");
    assert_eq!(ask(&mut rig, Request::ImportWebControl(json!([]))).0, Reply::Failed);
}

/// Upstream #90 and #135: an import tells the user which values it converted or kept.
#[test]
fn an_import_alerts_the_values_it_converted_or_kept() {
    let mut rig = Rig::connected();
    let ini = "[Maslow Settings]\nzAxis = 1.0\nzAxisSafeHeight = 0,01\nsledWeight = heavy\n";
    let (reply, effects) = ask(&mut rig, Request::ImportIni(ini.into()));
    assert_eq!(reply, Reply::Done);
    let alert = "The import did not take every value as written. Read zAxisSafeHeight \"0,01\" as 0.01. \
                 Kept sledWeight as it was: \"heavy\" is not a number.";
    assert_eq!(pushed(&effects, "activateModal")[0]["message"], alert);
    let settings = rig.machine.snapshot().settings;
    assert_eq!(settings.value("Maslow Settings", "zAxis"), Some(&json!(1)));
    assert_eq!(settings.value("Maslow Settings", "zAxisSafeHeight"), Some(&json!("0.01")));
    let refused = json!({"Maslow Settings": [{"key": "zAxisSafeHeight", "value": "abc"}]});
    let (reply, effects) = ask(&mut rig, Request::ImportWebControl(refused));
    assert_eq!(reply, Reply::Done);
    let alert = "The import did not take every value as written. Kept zAxisSafeHeight as it was: \"abc\" is not a \
                 number.";
    assert_eq!(pushed(&effects, "activateModal")[0]["message"], alert);
}

fn computed(rig: &Rig, key: &str) -> Value {
    rig.machine.snapshot().settings.value("Computed Settings", key).cloned().unwrap()
}

/// `importGCini` and `reloadWebControlJSON` run `computeSettings`, which writes the derived settings that changed.
#[test]
fn a_groundcontrol_import_derives_the_computed_settings_and_writes_them_before_dollar_dollar() {
    let mut rig = Rig::connected();
    rig.clear_written();
    let ini =
        "[Advanced Settings]\ngearTeeth = 9\nkinematicsType = Quadrilateral\n\n[Computed Settings]\ndistPerRot = 70\n";
    assert_eq!(ask(&mut rig, Request::ImportIni(ini.into())).0, Reply::Done);
    assert_eq!(computed(&rig, "distPerRot"), "57.15");
    assert_eq!(computed(&rig, "kinematicsTypeComputed"), "1");
    assert_eq!(rig.run_to_quiet(), ["$7=1", "$13=57.150", "$$"]);
}

#[test]
fn a_restore_derives_the_computed_settings_from_the_restored_file() {
    let mut rig = Rig::connected();
    rig.clear_written();
    let mut restored = Settings::defaults();
    restored.set_value("Advanced Settings", "chainOverSprocket", json!("Top"));
    assert_eq!(ask(&mut rig, Request::RestoreSettings(Box::new(restored))).0, Reply::Done);
    assert_eq!(computed(&rig, "chainOverSprocketComputed"), 1);
    assert_eq!(rig.run_to_quiet(), ["$38=1"], "the restart $38 needs replaces the read-back");
    assert_eq!(rig.reboot(), ["B05", "G20", "G90", "$$", "$13=63.5"], "the restored file is in inches");
}

/// Python's `importWebControlJSON` never recomputes; Rust does, so an imported file cannot leave them stale.
#[test]
fn a_webcontrol_json_import_derives_the_computed_settings_too() {
    let mut rig = Rig::connected();
    rig.clear_written();
    let imported = json!({"Advanced Settings": [{"key": "fPWM", "value": "4,100Hz"}]});
    assert_eq!(ask(&mut rig, Request::ImportWebControl(imported)).0, Reply::Done);
    assert_eq!(computed(&rig, "fPWMComputed"), 2);
    assert_eq!(rig.run_to_quiet(), ["$39=2", "$$"]);
    let stale = json!({"Computed Settings": [{"key": "distPerRot", "value": "63.5"}]});
    assert_eq!(ask(&mut rig, Request::ImportWebControl(stale)).0, Reply::Done);
    assert!(rig.run_to_quiet().is_empty(), "an import that changes nothing writes nothing");
}

#[test]
fn an_empty_gcode_has_no_lines_and_an_empty_preview() {
    let empty = Gcode::empty(Units::Mm);
    assert!(empty.lines.is_empty());
    assert_eq!(empty.z_moves, [0]);
    assert!(empty.preview.compressed.is_some());
}

#[test]
fn a_job_streams_with_the_files_truncate_in_the_home_shift() {
    let truncating = settings(&[
        ("Advanced Settings", "truncate", json!(1)),
        ("Advanced Settings", "digits", json!("2")),
        ("Advanced Settings", "homeX", json!("1.555")),
    ]);
    let mut rig = Rig::connected_with(truncating.clone());
    let gcode = Gcode::parse("G90\nG0 X1.5", &truncating).unwrap();
    ask(&mut rig, Request::LoadGcode { gcode: Box::new(gcode), file: None, origin: Origin::Open });
    rig.act("startRun");
    assert_eq!(rig.run_to_quiet(), ["G90", "G0 X3.06"]);
}

#[test]
fn a_parsed_file_is_estimated_at_the_machines_feeds() {
    let machine = settings(&[
        ("Advanced Settings", "maxFeedrate", json!("400")),
        ("Advanced Settings", "maxZRPM", json!("")),
        ("Maslow Settings", "zDistPerRot", json!("-24")),
    ]);
    let gcode = Gcode::parse("G0 X800\nG1 Z-10 F1000\n", &machine).unwrap();
    let z_seconds = 10.0 / (12.6 * 24.0) * 60.0;
    let g0 = 120.0 + MOVE_S;
    assert!((f64::from(gcode.estimate[0]) - g0).abs() < 0.001, "G0 at maxFeedrate");
    let z = f64::from(gcode.estimate[1]) - g0 - MOVE_S;
    assert!((z - z_seconds).abs() < 0.01, "Z at maxZRPM's default times the pitch");
    assert!(Gcode::empty(Units::Mm).estimate.is_empty());
}

#[test]
fn starting_a_run_remembers_the_file_and_its_home_in_inches() {
    let homes = [("Advanced Settings", "homeX", json!("254.0")), ("Advanced Settings", "homeY", json!("-127.0"))];
    let mut rig = Rig::connected_with(settings(&homes));
    open(&mut rig, SQUARE, "/lib/square.nc");
    rig.act("startRun");
    let last = |rig: &Rig, key| rig.machine.snapshot().settings.value("Computed Settings", key).cloned();
    assert_eq!(last(&rig, "lastRunFile"), Some(json!("/lib/square.nc")));
    assert_eq!((last(&rig, "lastRunHomeX"), last(&rig, "lastRunHomeY")), (Some(json!(10.0)), Some(json!(-5.0))));
    let mut unnamed = Rig::connected_with(settings(&[("Computed Settings", "lastRunFile", json!("/lib/old.nc"))]));
    unnamed.machine.load_gcode("G0 X1\n");
    unnamed.act("startRun");
    assert_eq!(last(&unnamed, "lastRunFile"), Some(json!("")), "gcode from no file leaves no ghost");
    rig.act("stopRun");
    rig.run_to_quiet();
    let edit = Request::LoadGcode { gcode: Box::new(parse("G0 X1")), file: None, origin: Origin::Edit };
    assert_eq!(ask(&mut rig, edit).0, Reply::Done);
    rig.act("startRun");
    assert_eq!(last(&rig, "lastRunFile"), Some(json!("")), "an edit no longer matches the file on disk");
}
