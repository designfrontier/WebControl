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
fn an_edit_keeps_the_index_where_it_fits_and_pushes_the_preview() {
    let mut rig = Rig::new();
    open(&mut rig, SQUARE, "square.nc");
    rig.act_with("moveGcodeGoto", json!(8), json!(""));
    let edit = Request::LoadGcode { gcode: Box::new(parse("G0 X1\nG0 X2")), file: None, origin: Origin::Edit };
    let (reply, effects) = ask(&mut rig, edit);
    assert_eq!(reply, Reply::Done);
    assert_eq!(rig.machine.snapshot().gcode_index, 1);
    assert_eq!(rig.machine.snapshot().gcode_file, Some(PathBuf::from("square.nc")), "an edit keeps the name");
    assert_eq!(commands(&effects), ["gcodePositionMessage", "showFPSpinner", "gcodeUpdateCompressed"]);
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
fn a_file_too_large_to_draw_raises_pythons_alert() {
    let mut rig = Rig::new();
    let mut gcode = parse("G0 X1");
    gcode.too_large = Some("only the first 300000lines are shown".into());
    let (_, effects) = ask(&mut rig, Request::LoadGcode { gcode: Box::new(gcode), file: None, origin: Origin::Open });
    assert_eq!(alerts(&effects), ["only the first 300000lines are shown"]);
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
    assert_eq!(commands(&effects), ["showFPSpinner", "gcodeUpdateCompressed"]);
    assert_eq!(inflate(&pushed(&effects, "gcodeUpdateCompressed")[0]), json!([]));
    assert_eq!(rig.machine.snapshot().gcode_count, 0);
    assert_eq!(rig.machine.snapshot().gcode_file, Some(PathBuf::from("square.nc")), "Python keeps the name");
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
    rig.machine
        .handle(Event::Request { id: 1, request: Request::LoadBoard(board_file("\"wide\"")) }, Default::default());
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
