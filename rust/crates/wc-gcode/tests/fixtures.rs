//! The Python-generated golden fixtures in tests/fixtures/gcode and tests/fixtures/boards (see the README there).

use std::io::Read;
use std::path::PathBuf;

use serde_json::{Value, json};
use wc_gcode::board::{Board, BoardEdit, BoardTrim};
use wc_gcode::firmware_gcode as fw;
use wc_gcode::position::find_position_at;
use wc_gcode::preview::{compress, render};
use wc_gcode::restart::{GcodeState, positioning_word, sequence, state_at, units_word};
use wc_gcode::sanitise::{sanitise, stream_problem, strip_comments};
use wc_gcode::shift::{HomeShift, move_line, positioning_set_by, shift_to_home};
use wc_gcode::{Positioning, Units, load};
use wc_testkit::contract::Contract;

fn fixtures() -> PathBuf {
    wc_testkit::repo_root().join("tests/fixtures")
}

fn read_json(relative: &str) -> Value {
    let path = fixtures().join(relative);
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn gcode_files() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(fixtures().join("gcode/files"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.ends_with(".nc"))
        .collect();
    names.sort();
    names
}

fn file_lines(name: &str, truncate: Option<usize>) -> Vec<String> {
    let raw = std::fs::read_to_string(fixtures().join("gcode/files").join(name)).unwrap();
    load::filter(&load::universal_newlines(&raw), truncate).unwrap()
}

/// Floats within `tolerance` (an integer and a float are both numbers), everything else exactly.
fn assert_matches(actual: &Value, expected: &Value, tolerance: f64, at: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(e)) => {
            let (a, e) = (a.as_f64().unwrap(), e.as_f64().unwrap());
            assert!((a - e).abs() <= tolerance, "{at}: {a} != {e}");
        }
        (Value::Array(a), Value::Array(e)) => {
            assert_eq!(a.len(), e.len(), "{at}: length");
            a.iter().zip(e).enumerate().for_each(|(i, (a, e))| assert_matches(a, e, tolerance, &format!("{at}[{i}]")));
        }
        (Value::Object(a), Value::Object(e)) => {
            assert_eq!(a.keys().collect::<Vec<_>>(), e.keys().collect::<Vec<_>>(), "{at}: keys");
            e.iter().for_each(|(key, e)| assert_matches(&a[key], e, tolerance, &format!("{at}.{key}")));
        }
        _ => assert_eq!(actual, expected, "{at}"),
    }
}

fn units(text: &Value) -> Units {
    if text == "MM" { Units::Mm } else { Units::Inches }
}

fn units_name(units: Units) -> &'static str {
    if units == Units::Mm { "MM" } else { "INCHES" }
}

fn positioning_code(positioning: Option<Positioning>) -> Value {
    positioning.map_or(Value::Null, |p| json!(if p == Positioning::Absolute { 0 } else { 1 }))
}

fn state_json(state: &GcodeState) -> Value {
    json!({
        "units": units_word(state.units), "positioning": positioning_word(state.positioning), "x": state.x, "y": state.y,
        "z": state.z, "feedRate": state.feed_rate, "zFeedRate": state.z_feed_rate, "motion": state.motion,
        "tool": state.tool.map(|tool| tool.to_string()), "spindle": state.spindle, "laser": state.laser,
        "dwell": state.dwell,
    })
}

fn inflate(bytes: &[u8]) -> Value {
    let mut json = String::new();
    flate2::read::GzDecoder::new(bytes).read_to_string(&mut json).unwrap();
    serde_json::from_str(&json).unwrap()
}

fn check_file(name: &str) {
    let fixture = read_json(&format!("gcode/{}.json", name.trim_end_matches(".nc")));
    let tolerance = fixture["tolerance"].as_f64().unwrap();
    let lines = file_lines(name, None);
    assert_eq!(json!(lines), fixture["lines"], "{name} lines");
    for digits in [4, 2] {
        assert_eq!(json!(file_lines(name, Some(digits))), fixture["truncated"][digits.to_string()], "{name} truncate");
    }
    for machine in [Units::Mm, Units::Inches] {
        let expected = &fixture[units_name(machine)];
        let z = load::z_moves(&lines, 0.5);
        assert_eq!(json!(z.indices), expected["zMoves"], "{name} zMoves");
        assert_eq!(json!(z.tolerance), expected["tolerance"], "{name} tolerance");
        let preview = render(&lines, machine);
        assert_eq!(units_name(preview.file_units), expected["fileUnits"], "{name} fileUnits");
        let moves = inflate(&compress(&preview.moves));
        assert_matches(&moves, &expected["moves"], tolerance, &format!("{name} moves"));
        Contract::shared().check("schemas/payloads/gcodeMoves.json", &moves).unwrap();
    }
    check_shifts(name, &fixture);
    check_restarts(name, &lines, &fixture, tolerance);
    check_positions(name, &lines, &fixture, tolerance);
}

fn check_shifts(name: &str, fixture: &Value) {
    fixture["shift"].as_array().unwrap().iter().for_each(|case| {
        let truncate = case["truncate"].as_i64().unwrap();
        let shift = HomeShift {
            x: case["x"].as_f64().unwrap(),
            y: case["y"].as_f64().unwrap(),
            truncate: u32::try_from(truncate).ok(),
        };
        case["moved"].as_array().unwrap().iter().for_each(|pair| {
            assert_eq!(move_line(pair[0].as_str().unwrap(), &shift), pair[1], "{name} {case:?}");
        });
    });
}

fn check_restarts(name: &str, lines: &[String], fixture: &Value, tolerance: f64) {
    fixture["restart"].as_array().unwrap().iter().for_each(|case| {
        let at = format!(
            "{name} restart {}",
            serde_json::to_string(&json!([
                &case["units"],
                &case["home"],
                &case["currentTool"],
                &case["index"],
                &case["zTarget"]
            ]))
            .unwrap()
        );
        let index = case["index"].as_u64().unwrap() as usize;
        let home = (case["home"]["x"].as_f64().unwrap(), case["home"]["y"].as_f64().unwrap());
        let state = state_at(lines, index, home.0, home.1, units(&case["units"]));
        let Some(expected) = case.get("state") else {
            assert!(state.is_err(), "{at}: expected {}", case["error"]);
            return;
        };
        let state = state.unwrap_or_else(|e| panic!("{at}: {e}"));
        assert_matches(&state_json(&state), expected, tolerance, &at);
        let restart = sequence(
            &state,
            case["zTarget"].as_f64(),
            case["changeTool"].as_bool().unwrap(),
            case["currentTool"].as_i64().unwrap(),
            5.0,
        );
        assert_eq!(json!(restart.lines), case["sent"], "{at}");
        assert_eq!(json!(restart.stopped_at_tool_change), case["stoppedAtToolChange"], "{at}");
    });
}

fn check_positions(name: &str, lines: &[String], fixture: &Value, tolerance: f64) {
    fixture["position"].as_array().unwrap().iter().for_each(|case| {
        let index = case["index"].as_u64().unwrap() as usize;
        let found = find_position_at(lines, index);
        match case.get("position") {
            Some(expected) => {
                let position = found.unwrap();
                let actual = json!({"position": [position.x, position.y, position.z], "feedRate": position.feed_rate});
                let expected = json!({"position": expected, "feedRate": case["feedRate"]});
                assert_matches(&actual, &expected, tolerance, &format!("{name} position {index}"));
            }
            None => assert!(found.is_err(), "{name} position {index}"),
        }
    });
}

#[test]
fn every_gcode_file_matches_its_python_fixture() {
    let names = gcode_files();
    assert!(names.len() >= 12, "{names:?}");
    names.iter().for_each(|name| check_file(name));
}

#[test]
fn the_streamed_line_checks_match_python() {
    let fixture = read_json("gcode/stream.json");
    let home = HomeShift { x: 41.6763, y: 60.1447, truncate: None };
    fixture["lines"].as_array().unwrap().iter().for_each(|case| {
        let raw = case["raw"].as_str().unwrap();
        let line = sanitise(raw);
        let tool_changes: Vec<Value> = [(0, 0), (2, 0), (2, 2), (3, 1)]
            .iter()
            .map(|&(next, current)| {
                let change = fw::tool_change_line(&line, next, current);
                json!([next, current, change.line, change.next_tool, change.pauses])
            })
            .collect();
        let actual = json!({
            "raw": raw, "stripped": strip_comments(raw), "sanitised": line,
            "positioningMode": positioning_code(positioning_set_by(&line)),
            "pauses": fw::pauses_firmware(&line), "changesTool": fw::changes_tool(&line),
            "onlySelectsTool": fw::only_selects_tool(&line), "toolNamed": fw::tool_named_by(&line),
            "toolFirmwareReads": fw::tool_the_firmware_reads(&line),
            "toolChange": tool_changes,
            "mode0": stream_entry(&line, Positioning::Absolute, &home),
            "mode1": stream_entry(&line, Positioning::Relative, &home),
        });
        assert_eq!(actual, *case, "{raw:?}");
    });
}

fn stream_entry(line: &str, mode: Positioning, home: &HomeShift) -> Value {
    let to_send = fw::tool_change_line(line, 0, 0).line;
    json!({
        "shifted": shift_to_home(line, mode, home),
        "problem": stream_problem(&to_send, mode, home).map(|problem| problem.title()),
    })
}

fn board_from(spec: &Value) -> Board {
    let text = |key: &str| spec[key].to_string();
    let (height, width, center_x, center_y) = (text("height"), text("width"), text("centerX"), text("centerY"));
    let mut board = Board::for_bed(0.0, 0.0);
    let edit = BoardEdit {
        board_id: "-",
        material: "-",
        height: &height,
        width: &width,
        thickness: "0.75",
        center_x: &center_x,
        center_y: &center_y,
        units: "inches",
    };
    assert!(board.update_info(&edit));
    board
}

fn shift_of(value: &Value) -> (f64, f64) {
    (value[0].as_f64().unwrap(), value[1].as_f64().unwrap())
}

fn cells_json(board: &Board) -> Value {
    let cells = json!(board.cut_points);
    Contract::shared().check("schemas/payloads/cutCells.json", &cells).unwrap();
    assert_eq!(board.cut_payload().map(|payload| inflate(&payload)), Some(cells.clone()));
    cells
}

#[test]
fn board_cut_grids_match_python() {
    let fixture = read_json("boards/cut_grids.json");
    fixture["cases"].as_array().unwrap().iter().for_each(|case| {
        let machine = units(&case["units"]);
        let moves = render(&file_lines(case["file"].as_str().unwrap(), None), machine).moves;
        let mut board = board_from(&case["board"]);
        let (x, y) = shift_of(&case["shift"]);
        board.process_gcode(&moves, (x, y), machine).unwrap();
        assert_eq!(cells_json(&board), case["cut"], "{case:?}");
        board.process_gcode(&moves, (x + 5.0, y - 3.0), machine).unwrap();
        assert_eq!(cells_json(&board), case["cutAgainShifted"], "{case:?}");
    });
}

#[test]
fn board_trims_match_python() {
    let fixture = read_json("boards/trim.json");
    let tolerance = fixture["tolerance"].as_f64().unwrap();
    let moves = render(&file_lines("vcarve_mm.nc", None), Units::Mm).moves;
    fixture["cases"].as_array().unwrap().iter().for_each(|case| {
        let mut board = board_from(&case["board"]);
        board.process_gcode(&moves, (-100.0, -50.0), Units::Mm).unwrap();
        assert_eq!(cells_json(&board), case["before"]);
        let trim = &case["trim"];
        let text = |key: &str| trim[key].as_str().unwrap();
        let edges = BoardTrim {
            top: text("top"),
            bottom: text("bottom"),
            left: text("left"),
            right: text("right"),
            units: text("units"),
        };
        board.trim(&edges).unwrap();
        let after = json!({"info": board.info(), "cutPoints": cells_json(&board)});
        assert_matches(&after, &case["after"], tolerance, &format!("{trim}"));
    });
}

#[test]
fn board_info_edits_and_new_boards_match_python() {
    let fixture = read_json("boards/info.json");
    let tolerance = fixture["tolerance"].as_f64().unwrap();
    let mut board = Board::for_bed(0.0, 0.0);
    fixture["edits"].as_array().unwrap().iter().for_each(|case| {
        let edit = &case["edit"];
        let text = |key: &str| edit[key].as_str().unwrap();
        let accepted = board.update_info(&BoardEdit {
            board_id: text("boardID"),
            material: text("material"),
            height: text("height"),
            width: text("width"),
            thickness: text("thickness"),
            center_x: text("centerX"),
            center_y: text("centerY"),
            units: text("units"),
        });
        assert_eq!(json!(accepted), case["accepted"], "{edit}");
        assert_matches(&board.info(), &case["info"], tolerance, &format!("{edit}"));
        Contract::shared().validate_message("boardDataUpdate", &board.info()).unwrap();
    });
    fixture["newBoards"].as_array().unwrap().iter().for_each(|case| {
        let bed = |key: &str| case[key].as_str().unwrap().parse::<f64>().unwrap();
        let board = Board::for_bed(bed("bedWidth"), bed("bedHeight"));
        assert_matches(&board.info(), &case["info"], tolerance, "new board");
        assert_eq!(cells_json(&board), case["cutPoints"]);
        assert_eq!(json!(board.cut_points), case["cutData"]);
    });
}

#[test]
fn board_files_match_python() {
    let fixture = read_json("boards/files.json");
    let moves = render(&file_lines("square_mm.nc", None), Units::Mm).moves;
    fixture["cases"].as_array().unwrap().iter().for_each(|case| {
        let name = case["name"].as_str().unwrap();
        let mut board = Board::for_bed(2438.4, 1219.2);
        let loaded = board.load(case["text"].as_str().unwrap()).is_ok();
        assert_eq!(json!(loaded), case["loaded"], "{name}");
        let state = &case["state"];
        assert_eq!(board.info(), state["info"], "{name}");
        assert_matches(&json!(board.cut_points), &state["cutPoints"], 0.0, name);
        if let Some(expected) = case.get("afterProcess") {
            board.process_gcode(&moves, (0.0, 0.0), Units::Mm).unwrap();
            assert_matches(&json!(board.cut_points), expected, 0.0, name);
        }
        let text = board.save("out.brd").unwrap();
        let (info_line, cut_line) = text.split_once('\n').unwrap();
        assert_eq!(info_line, case["saved"]["infoLine"], "{name}");
        let mut reloaded = Board::for_bed(1.0, 1.0);
        reloaded.load(&text).unwrap();
        assert_matches(&json!(reloaded.cut_points), &case["saved"]["cutData"], 0.0, &format!("{name} {cut_line}"));
    });
}
