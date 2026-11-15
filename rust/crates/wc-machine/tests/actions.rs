//! Actions and their gates, and the UI's pushes (ported from test_actions_characterization, test_motion,
//! test_ui_processor and test_api_main).

mod rig;

use rig::{Rig, alerts, commands, messages, pushed, settings, written};
use serde_json::json;
use wc_machine::{Effect, Event};

#[test]
fn unknown_and_later_commands_raise_pythons_alert() {
    let mut rig = Rig::new();
    assert_eq!(alerts(&rig.act("noSuchCommand")), ["Function not currently implemented.. Sorry.[noSuchCommand]"]);
}

#[test]
fn machine_commands_need_the_controller() {
    let mut rig = Rig::new();
    ["move", "home", "touchZ", "macro1", "reportSettings"]
        .iter()
        .for_each(|command| assert_eq!(alerts(&rig.act(command)), ["The controller is not connected."], "{command}"));
}

/// Ported from test_actions_characterization: jogs queue a relative rapid and remember the distance.
#[test]
fn a_jog_is_a_relative_rapid_and_remembers_the_distance() {
    let mut rig = Rig::connected();
    rig.act_with("move", json!("upRight"), json!(10));
    assert_eq!(rig.run_to_quiet(), ["G91", "G00 X10.0 Y10.0", "G90"]);
    assert_eq!(rig.machine.snapshot().settings.text("Computed Settings", "distToMove").as_deref(), Some("10.0"));
    let mut diagonal = Rig::connected_with(settings(&[("WebControl Settings", "diagonalMove", json!(1))]));
    diagonal.act_with("move", json!("downLeft"), json!("10"));
    assert_eq!(diagonal.run_to_quiet(), ["G91", "G00 X-7.0711 Y-7.0711", "G90"]);
}

#[test]
fn a_bad_jog_sends_nothing() {
    let mut rig = Rig::connected();
    assert_eq!(alerts(&rig.act_with("move", json!("sideways"), json!(10))), ["Error with initiating move."]);
    assert!(messages(&rig.act_with("move", json!("up"), json!("far"))).is_empty(), "Python fails quietly on float()");
    assert!(rig.run_to_quiet().is_empty());
}

#[test]
fn move_to_takes_inches_and_stays_on_the_bed() {
    let mut rig = Rig::connected();
    rig.act_with("moveTo", json!(10), json!("5"));
    assert_eq!(rig.run_to_quiet(), ["G90 G00 X254.0 Y127.0"]);
    assert_eq!(alerts(&rig.act_with("moveTo", json!(1000), json!(0))), ["Error with initiating move."]);
    assert_eq!(alerts(&rig.act_with("moveTo", json!("10"), json!(0))), ["Error with initiating move."]);
}

/// Ported from test_motion: Z moves go out in the machine's units.
#[test]
fn z_moves_convert_the_panels_units() {
    let mut rig = Rig::connected_with(settings(&[("Computed Settings", "unitsZ", json!("INCHES"))]));
    rig.act_with("moveZ", json!("lower"), json!("0.5"));
    assert_eq!(rig.run_to_quiet(), ["G21", "G91", "G00 Z-12.7", "G90"]);
    rig.act_with("moveZ", json!("nowhere"), json!("1"));
    assert_eq!(rig.run_to_quiet(), ["G21"], "Python still sets the units");
    assert_eq!(rig.machine.snapshot().settings.text("Computed Settings", "distToMoveZ").as_deref(), Some("1.0"));
}

#[test]
fn home_and_center_rise_to_the_safe_height_first() {
    let mut rig = Rig::connected_with(settings(&[("Advanced Settings", "homeX", json!("12.5"))]));
    rig.act("home");
    assert_eq!(rig.run_to_quiet(), ["G90", "G00 Z5.0", "G00 X12.5 Y0.0", "G00 Z0"]);
    rig.act("returnToCenter");
    assert_eq!(rig.run_to_quiet(), ["G90", "G00 Z5.0", "G00 X0.0 Y0.0"]);
    let mut broken = Rig::connected_with(settings(&[("Maslow Settings", "zAxisSafeHeight", json!("high"))]));
    assert_eq!(alerts(&broken.act("home")), ["Error with returning to home."]);
    assert_eq!(alerts(&broken.act("returnToCenter")), ["Error with returning to center"]);
}

#[test]
fn other_manual_commands_queue_their_lines() {
    let mut rig = Rig::connected();
    rig.act("moveToDefault");
    assert_eq!(rig.run_to_quiet(), ["G90", "B09 R1651 L1651", "G91", "B08"]);
    rig.act("defineZ0");
    assert_eq!(rig.run_to_quiet(), ["G10 Z0"]);
    rig.act("reportSettings");
    assert_eq!(rig.run_to_quiet(), ["$$"]);
}

#[test]
fn macros_send_their_text_with_the_tool_first_and_follow_their_units() {
    let mut rig = Rig::connected_with(settings(&[
        ("Maslow Settings", "macro1", json!("G0 Z10 (lift)")),
        ("Maslow Settings", "macro2", json!("M6 T2")),
    ]));
    rig.act("macro1");
    assert_eq!(rig.run_to_quiet(), ["G0 Z10"]);
    rig.act("macro2");
    assert_eq!(rig.run_to_quiet(), ["T2 M6"]);
    assert_eq!(rig.machine.snapshot().current_tool, 2);
    let mut units = Rig::connected_with(settings(&[("Maslow Settings", "macro1", json!("G20 G0 X1"))]));
    units.act("macro1");
    assert_eq!(units.run_to_quiet(), ["G20 G0 X1"]);
    assert_eq!(units.machine.snapshot().units(), wc_protocol::main::Units::Inches);
    let mut empty = Rig::connected_with(settings(&[("Maslow Settings", "macro1", json!("(nothing)"))]));
    empty.act("macro1");
    assert!(empty.run_to_quiet().is_empty());
}

/// The firmware runs a lone T word as a modal move, so a command that only selects a tool is dropped.
#[test]
fn a_macro_that_only_selects_a_tool_is_not_sent() {
    let mut rig = Rig::connected_with(settings(&[
        ("Maslow Settings", "macro1", json!("T2")),
        ("Maslow Settings", "macro2", json!("G0 Z10")),
    ]));
    rig.act("macro1");
    rig.act("macro2");
    assert_eq!(rig.run_to_quiet(), ["G0 Z10"]);
}

/// Ported from test_motion: Touch Z probes in the machine's units, and every moving action waits for it.
#[test]
fn touch_z_refuses_other_actions_until_the_probe_answers() {
    let mut rig = Rig::connected_with(settings(&[("Computed Settings", "units", json!("INCHES"))]));
    assert_eq!(written(&rig.act("touchZ")), ["G90 G38.2 Z-0.3937"]);
    assert_eq!(alerts(&rig.act("home")), ["Wait for Touch Z to finish or press Stop"]);
    assert!(alerts(&rig.act("statusRequest")).is_empty());
    rig.advance(100);
    rig.rx("z axis zeroed");
    assert!(alerts(&rig.rx("ok")).is_empty());
    assert!(alerts(&rig.act("home")).is_empty());
    let mut broken = Rig::connected_with(settings(&[("Advanced Settings", "maxTouchProbePlungeDistance", json!("x"))]));
    assert_eq!(alerts(&broken.act("touchZ")), ["Error with touching."]);
}

#[test]
fn stop_z_ends_a_probe_and_a_running_job() {
    let mut rig = Rig::connected();
    rig.act("touchZ");
    assert_eq!(written(&rig.act("stopZ")), ["!"]);
    assert!(alerts(&rig.act("home")).is_empty(), "Stop ends the probe");
    let mut job = Rig::connected();
    job.machine.load_gcode("G0 X1\nG0 X2\n");
    job.act("startRun");
    job.advance(100);
    job.act("stopZ");
    assert!(!job.machine.snapshot().job_in_progress);
}

/// Ported from test_actions_characterization: Define Home from the sled or from inches, and Reset to center.
#[test]
fn define_home_takes_the_sled_or_inches() {
    let mut rig = Rig::connected();
    rig.rx("<Idle,MPos:12.34,-5.50,0.00,WPos:0,0,0>");
    let effects = rig.act("defineHome");
    assert_eq!(pushed(&effects, "homePositionMessage"), [json!({"xval": 12.34, "yval": -5.5})]);
    assert_eq!(pushed(&effects, "requestedSetting"), [json!({"setting": "units", "value": "MM"})]);
    rig.act_with("defineHome", json!(10), json!("5"));
    let home = rig.machine.snapshot().settings;
    assert_eq!(
        (home.text("Advanced Settings", "homeX"), home.text("Advanced Settings", "homeY")),
        (Some("254.0".into()), Some("127.0".into()))
    );
    assert_eq!(alerts(&rig.act_with("defineHome", json!("x"), json!(1))), ["Error with defining home."]);
    let reset = rig.act("resetHomeToCenter");
    assert_eq!(pushed(&reset, "homePositionMessage"), [json!({"xval": 0.0, "yval": 0.0})]);
    assert!(pushed(&reset, "requestedSetting").is_empty(), "Python sends units only after Define Home");
    assert_eq!(pushed(&reset, "gcodeUpdateCompressed"), [json!("")]);
    assert!(reset.iter().any(|effect| matches!(effect, Effect::SaveSettings(_))), "home is saved");
}

#[test]
fn system_actions_reach_the_host() {
    let mut rig = Rig::new();
    assert!(rig.act("shutdown").contains(&Effect::Exit));
    assert!(rig.act("TurnOffRPI").contains(&Effect::PowerOff));
    assert!(rig.act("updatePorts").contains(&Effect::ListPorts));
    assert!(rig.act("clearLogs").contains(&Effect::ClearLogs));
    assert!(messages(&rig.act_with("statusRequest", json!("cameraStatus"), json!(""))).is_empty());
}

/// Ported from test_actions_characterization: while streaming only the `always` actions run.
#[test]
fn streaming_refuses_everything_but_the_always_actions() {
    let mut rig = Rig::connected();
    assert_eq!(alerts(&rig.act("startRun")), ["No GCode file loaded."]);
    rig.machine.load_gcode("G0 X1\nG0 X2\n");
    rig.act("startRun");
    let snapshot = rig.machine.snapshot();
    assert_eq!((snapshot.upload_flag, snapshot.job_in_progress), (1, true));
    ["move", "startRun", "defineHome", "updatePorts"]
        .iter()
        .for_each(|c| assert_eq!(alerts(&rig.act(c)), ["Cannot issue command while sending gcode."], "{c}"));
    assert!(alerts(&rig.act("statusRequest")).is_empty());
    assert!(written(&rig.act("stopRun")).contains(&"!".to_string()));
}

/// Ported from test_message_processor: the firmware drops its buffer on an ALARM, so the job rewinds.
#[test]
fn an_alarm_rewinds_to_the_oldest_line_not_acknowledged() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode("G0 X1\nG0 X2\nG0 X3\n");
    rig.act("startRun");
    rig.advance(100);
    let effects = rig.rx("ALARM: The sled is not keeping up");
    assert_eq!(pushed(&effects, "alarm"), [json!({"message": "Alarm: Sled Not Keeping Up"})]);
    let snapshot = rig.machine.snapshot();
    assert_eq!((snapshot.gcode_index, snapshot.job_in_progress, snapshot.upload_flag), (0, true, 0));
    assert!(written(&rig.advance(300)).is_empty());
    assert!(written(&rig.rx("ok")).is_empty(), "a stale ack releases nothing");
    rig.rx("<Idle,MPos:0.00,0.00,0.00,WPos:0,0,0>");
    assert_eq!(written(&rig.act("defineZ0")), ["G10 Z0"], "a status report after the input ran dry ends the resync");
}

#[test]
fn play_after_an_overflow_returns_to_the_dropped_line_at_safe_height() {
    let mut moved = Rig::connected();
    moved.machine.load_gcode("G0 X1\nG0 X2\nG0 X3\n");
    moved.act("startRun");
    moved.advance(100);
    moved.rx("ok");
    moved.advance(100);
    moved.rx("Buffer overflow!");
    assert_eq!(moved.machine.snapshot().gcode_index, 1);
    moved.advance(300);
    moved.rx("<Idle,MPos:0.00,0.00,0.00,WPos:0,0,0>");
    assert_eq!(written(&moved.act("startRun")), ["~"]);
    let sent = moved.run_to_quiet();
    assert!(sent.ends_with(&["~", "G21", "G90", "G0 Z5.0", "G0 X1.0 Y0.0", "G90", "G0 X2", "G0 X3"].map(String::from)));
}

/// Ported from test_serial_streaming: one line per ok, comments and blank lines skipped, the home shift applied.
#[test]
fn a_job_streams_one_line_per_ok_and_ends() {
    let mut rig = Rig::connected_with(settings(&[("Advanced Settings", "homeX", json!("1.5"))]));
    rig.machine.load_gcode("(start)\nG0 X1 ; go\n\nT2\nG91\nG0 X1\n");
    rig.act("startRun");
    assert_eq!(rig.run_to_quiet(), ["G0 X2.5", "G91", "G0 X1"]);
    let snapshot = rig.machine.snapshot();
    assert_eq!((snapshot.job_in_progress, snapshot.gcode_index), (false, 0));
}

/// Ported from test_serial_streaming: a line the controller cannot take pauses the job before it, with an alert.
#[test]
fn lines_the_controller_cannot_take_pause_the_job_with_an_alert() {
    let long = format!("G1 X1 {}", "F100 ".repeat(25));
    let cases = [
        ("G0 X1\nG0 (unclosed\n".to_string(), "Line Unclosed Comment", "Line 2 has an unclosed comment"),
        (format!("G0 X1\n{long}\n"), "Line Too Long", "Line 2 is 13"),
    ];
    cases.iter().for_each(|(file, title, message)| {
        let mut rig = Rig::connected();
        rig.machine.load_gcode(file);
        let mut effects = rig.act("startRun");
        effects.extend(rig.advance(100));
        effects.extend(rig.rx("ok"));
        effects.extend(rig.advance(100));
        let modal = pushed(&effects, "activateModal");
        assert_eq!(modal.last().map(|m| m["title"].clone()), Some(json!(title)), "{file}");
        assert!(modal.last().unwrap()["message"].as_str().unwrap().starts_with(message), "{file}");
        let snapshot = rig.machine.snapshot();
        assert_eq!((snapshot.upload_flag, snapshot.gcode_index, snapshot.job_in_progress), (2, 1, true));
    });
}

/// Ported from test_ui_processor: position reports go out when the sled moves, with default power and feed.
#[test]
fn position_reports_are_sent_when_the_sled_moves() {
    let mut rig = Rig::connected();
    rig.rx("<Idle,MPos:1.00,0.00,0.00,WPos:0,0,0>");
    let first = pushed(&rig.rx("<Idle,MPos:2.00,0.00,0.00,WPos:0,0,0>"), "positionMessage");
    assert_eq!(first.len(), 1);
    assert_eq!((first[0]["lpwr"].clone(), first[0]["rqvel"].clone()), (json!(15.0), json!(20.0)));
    assert_eq!((first[0]["pcom"].clone(), first[0]["state"].clone()), (json!("0.0%"), json!("Idle")));
    let stopped = pushed(&rig.rx("<Idle,MPos:2.00,0.00,0.00,WPos:0,0,0>"), "positionMessage");
    assert_eq!(stopped[0]["vel"], 0.0, "once more when it stops");
    assert!(pushed(&rig.rx("<Idle,MPos:2.00,0.00,0.00,WPos:0,0,0>"), "positionMessage").is_empty());
    let nan = rig.rx("<Stop,MPos:nan,0.00,0.00,WPos:0,0,0>");
    assert_eq!(pushed(&nan, "controllerMessage"), [json!("Unable to resolve x Kinematics.")]);
    assert_eq!(pushed(&nan, "positionMessage")[0]["state"], "Stopped");
}

/// Ported from test_ui_processor: PE reports by field count, with the sled position the chain lengths give.
#[test]
fn error_reports_are_fractions_of_the_limit() {
    let mut rig = Rig::connected_with(settings(&[("WebControl Settings", "computedPosition", json!(1))]));
    let holey = pushed(&rig.rx("[PE:0.5,-1.0,120,1650.0,1651.0]"), "errorValueMessage");
    assert_eq!(holey.len(), 1);
    assert_eq!(holey[0]["leftError"], 0.25);
    assert_eq!(holey[0]["rightError"], 0.5);
    assert_eq!(holey[0]["computedEnabled"], 1);
    // Float32 as the firmware computes (owner decision 6): within 0.005 mm of Python's f64
    // HoleySimulationKinematics.forward(1650, 1651, 0, 0) on the default settings, but not equal to it.
    let (x, y) = (holey[0]["computedX"].as_f64().unwrap(), holey[0]["computedY"].as_f64().unwrap());
    assert_eq!((x as f32 as f64, y as f32 as f64), (x, y), "float32 values");
    assert!((x - -0.5903697011988455).abs() < 5e-3 && (y - 126.41059578749022).abs() < 5e-3, "({x}, {y})");
    assert_ne!((x, y), (-0.5903697011988455, 126.41059578749022));
    assert!(pushed(&rig.rx("[PE:0.5001,-1.0,120,1650.0,1651.0]"), "errorValueMessage").is_empty());
    let short = pushed(&rig.rx("[PE:0.1,0.1,120]"), "errorValueMessage");
    assert_eq!((&short[0]["computedEnabled"], &short[0]["computedX"]), (&json!(0), &json!(-999999.0)));
    let mut off = Rig::connected_with(settings(&[("Advanced Settings", "positionErrorLimit", json!("0"))]));
    assert!(pushed(&off.rx("[PE:0.5,-1.0,120]"), "errorValueMessage").is_empty());
}

#[test]
fn the_computed_position_is_left_out_when_disabled_or_unreachable() {
    let mut off = Rig::connected();
    let report = pushed(&off.rx("[PE:0.5,-1.0,120,1650.0,1651.0]"), "errorValueMessage");
    assert_eq!((&report[0]["computedEnabled"], &report[0]["computedX"]), (&json!(0), &json!(-999999.0)));
    let mut on = Rig::connected_with(settings(&[("WebControl Settings", "computedPosition", json!(1))]));
    let unreachable = pushed(&on.rx("[PE:0.5,-1.0,120,10.0,9000.0]"), "errorValueMessage");
    assert_eq!((&unreachable[0]["computedX"], &unreachable[0]["computedY"]), (&json!(-999999.0), &json!(-999999.0)));
}

/// Every `$` write updates the model, as `config.setValue` called `holeyKinematics.updateSetting`.
#[test]
fn a_setting_written_to_the_controller_moves_the_computed_position() {
    let mut rig = Rig::connected_with(settings(&[("WebControl Settings", "computedPosition", json!(1))]));
    let before = pushed(&rig.rx("[PE:0.5,-1.0,120,1650.0,1651.0]"), "errorValueMessage")[0]["computedY"].clone();
    let body = json!({"values": {"motorOffsetY": "500"}});
    rig.send(Event::Edit { id: 1, request: wc_machine::Request::UpdateSettings { section: "maslow".into(), body } });
    assert_eq!(rig.run_to_quiet(), ["$3=500"]);
    let after = pushed(&rig.rx("[PE:0.1,-1.0,120,1650.0,1651.0]"), "errorValueMessage")[0]["computedY"].clone();
    let lift = after.as_f64().unwrap() - before.as_f64().unwrap();
    assert!((lift - 37.0).abs() < 1.0, "the sled computes about 37 mm higher, not {lift}");
}

#[test]
fn quadrilateral_settings_give_a_quadrilateral_model() {
    let quadrilateral = settings(&[
        ("WebControl Settings", "computedPosition", json!(1)),
        ("Computed Settings", "kinematicsTypeComputed", json!("1")),
        ("Computed Settings", "chainOverSprocketComputed", json!("junk")),
    ]);
    let mut rig = Rig::connected_with(quadrilateral);
    let report = pushed(&rig.rx("[PE:0.5,-1.0,120,1650.0,1651.0]"), "errorValueMessage");
    assert!(report[0]["computedX"].as_f64().unwrap() > -999999.0, "{report:?}");
}

#[test]
fn the_status_message_goes_out_when_it_changes() {
    let mut rig = Rig::connected();
    let effects = rig.act_with("move", json!("up"), json!(1));
    assert_eq!(pushed(&effects, "statusMessage")[0]["positioningMode"], 1);
    assert!(pushed(&rig.advance(10), "statusMessage").is_empty());
}

/// Ported from test_api_settings' settingRequest half: every answer is a broadcast `requestedSetting`.
#[test]
fn setting_requests_answer_with_the_stored_value() {
    let mut rig = Rig::new();
    let request = |rig: &mut Rig, section: &str, setting: &str| {
        rig.send(Event::SettingRequest { section: section.into(), setting: setting.into() })
    };
    assert_eq!(
        pushed(&request(&mut rig, "Computed Settings", "units"), "requestedSetting"),
        [json!({"setting": "units", "value": "MM"})]
    );
    assert_eq!(
        pushed(&request(&mut rig, "", "homePosition"), "homePositionMessage"),
        [json!({"xval": 0.0, "yval": 0.0})]
    );
    assert_eq!(pushed(&request(&mut rig, "", "pauseButtonSetting"), "requestedSetting")[0]["value"], "Pause");
    assert!(messages(&request(&mut rig, "Optical Calibration Settings", "calibrationCurve")).is_empty());
    assert!(messages(&request(&mut rig, "Nope", "nope")).is_empty());
    assert_eq!(commands(&request(&mut rig, "Computed Settings", "distToMove")), ["requestedSetting"]);
}
