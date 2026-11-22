//! Tool changes and the firmware's pauses, ported from test_tool_change: streaming stops exactly where the firmware
//! pauses, every M6 names its tool first, and only Resume (or a jog with no job lines behind it) releases the firmware.

mod rig;

use rig::{Rig, alerts, pushed, written};
use serde_json::json;
use wc_machine::Event;
use wc_machine::files::Request;

fn file(lines: &[&str]) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
}

/// Plays `lines` from the start, acking each line, until streaming stops; returns what was written.
fn stream(rig: &mut Rig, lines: &[&str]) -> Vec<String> {
    rig.machine.load_gcode(&file(lines));
    rig.clear_written();
    rig.act("startRun");
    rig.run_to_quiet()
}

fn job_state(rig: &Rig) -> (i32, u32, usize) {
    let snapshot = rig.machine.snapshot();
    (snapshot.upload_flag, snapshot.current_tool, snapshot.gcode_index)
}

fn tool_change(rig: &mut Rig, tool: u32) {
    rig.rx(&format!("Tool Change: Please insert tool {tool}"));
    rig.rx("Maslow Paused");
}

#[test]
fn a_lone_t_is_not_sent_and_leads_the_m6_the_firmware_would_skip() {
    let mut rig = Rig::connected();
    assert_eq!(stream(&mut rig, &["G1 X1", "T2", "M6", "G1 X2"]), ["G1 X1", "T2 M6"]);
    assert_eq!(job_state(&rig), (-1, 2, 3));
}

#[test]
fn a_line_that_only_selects_a_tool_is_not_sent() {
    ["T2", "T 3", "N10 T2", "T2.0"].iter().for_each(|line| {
        let mut rig = Rig::connected();
        assert_eq!(stream(&mut rig, &["G1 X1", line, "G1 X2"]), ["G1 X1", "G1 X2"], "{line}");
    });
}

#[test]
fn a_queued_lone_t_is_not_sent_but_selects_the_tool() {
    let mut rig = Rig::connected();
    rig.send(Event::Request { id: 1, request: Request::SendGcode("T2\nM6".into()) });
    assert_eq!(rig.run_to_quiet(), ["T2 M6"]);
}

/// File lines, what goes out, and the upload flag and tool after them.
type Case<'a> = (&'a [&'a str], &'a [&'a str], (i32, u32));

#[test]
fn every_m6_form_goes_out_led_by_the_tool_it_changes_to() {
    let cases: [Case; 6] = [
        (&["M6 T2", "G1 X2"], &["T2 M6"], (-1, 2)),
        (&["T3 M06", "G1 X2"], &["T3 M06"], (-1, 3)),
        (&["G0 X1 T2", "M6", "G1 X2"], &["G0 X1 T2", "T2 M6"], (-1, 2)),
        (&["N10 T2 M6", "G1 X2"], &["T2 N10 M6"], (-1, 2)),
        (&["M60", "B14 L1 S0 F1 I1 T2 V1", "M6", "G1 X2"], &["M60", "B14 L1 S0 F1 I1 T2 V1", "T0 M6", "G1 X2"], (0, 0)),
        (&["T3 G0 X1", "T0", "M6", "G1 X2"], &["T3 G0 X1", "T0 M6", "G1 X2"], (0, 0)),
    ];
    cases.iter().for_each(|(lines, sent, (flag, tool))| {
        let mut rig = Rig::connected();
        assert_eq!(stream(&mut rig, lines), *sent, "{lines:?}");
        assert_eq!(job_state(&rig).0, *flag, "{lines:?}");
        assert_eq!(job_state(&rig).1, *tool, "{lines:?}");
    });
}

#[test]
fn an_m6_for_the_tool_already_in_keeps_streaming() {
    let mut rig = Rig::connected();
    stream(&mut rig, &["T2 M6", "G1 X1"]);
    tool_change(&mut rig, 2);
    rig.act("stopRun");
    rig.advance(300);
    rig.rx("<Idle,MPos:0.00,0.00,0.00,WPos:0,0,0>");
    assert_eq!(stream(&mut rig, &["T2 M6", "G1 X2"]), ["T2 M6", "G1 X2"]);
    assert_eq!(job_state(&rig).0, 0);
}

#[test]
fn the_tool_change_prompt_leaves_the_firmware_paused_until_resume() {
    let mut rig = Rig::connected();
    stream(&mut rig, &["T2 M6", "G1 X2"]);
    let effects = rig.rx("Tool Change: Please insert tool 2");
    assert!(written(&effects).is_empty());
    rig.rx("Maslow Paused");
    let snapshot = rig.machine.snapshot();
    assert_eq!((snapshot.upload_flag, snapshot.job_in_progress, snapshot.manual_z_axis_adjust), (0, true, true));
    assert_eq!(written(&rig.act("resumeRun"))[0], "~");
    let snapshot = rig.machine.snapshot();
    assert_eq!((snapshot.upload_flag, snapshot.manual_z_axis_adjust, snapshot.gcode_index), (1, false, 1));
}

#[test]
fn jogging_during_a_tool_change_releases_the_firmware_once() {
    let mut rig = Rig::connected();
    stream(&mut rig, &["T2 M6", "G1 X2"]);
    tool_change(&mut rig, 2);
    assert_eq!(written(&rig.act_with("moveZ", json!("raise"), json!("5"))), ["~"]);
    assert!(written(&rig.act_with("moveZ", json!("lower"), json!("1"))).is_empty());
    let snapshot = rig.machine.snapshot();
    assert_eq!((snapshot.upload_flag, snapshot.manual_z_axis_adjust), (0, true));
}

#[test]
fn resume_after_jogging_during_a_tool_change_returns_to_the_file_at_safe_height() {
    let mut rig = Rig::connected();
    let lines = ["G21", "G90", "G0 Z5", "G0 X10 Y10", "G1 Z-3 F100", "G1 X20 F600", "G0 Z5", "T2 M6", "G0 X40 Y40"];
    stream(&mut rig, &lines);
    tool_change(&mut rig, 2);
    rig.clear_written();
    rig.act_with("moveTo", json!(0), json!(0));
    rig.act("resumeRun");
    let sent = rig.run_to_quiet();
    let expected =
        ["~", "~", "G90 G00 X0.0 Y0.0", "G21", "G90", "G0 Z5.0", "G0 X20.0 Y10.0", "G0 F600.0", "G90", "G0 X40 Y40"];
    assert_eq!(sent, expected);
}

#[test]
fn a_tool_change_never_lowers_the_new_bit_to_a_z_from_an_old_pause() {
    let mut rig = Rig::connected();
    let lines = ["G21", "G90", "G1 Z-3", "G0 X1", "T2 M6", "G0 X40 Y40", "G1 Z-2"];
    rig.machine.load_gcode(&file(&lines));
    rig.act("startRun");
    (0..3).for_each(|_| {
        rig.advance(60);
        rig.rx("ok");
    });
    rig.act("pauseRun");
    rig.rx("ok");
    rig.act("resumeRun");
    rig.run_to_quiet();
    tool_change(&mut rig, 2);
    rig.clear_written();
    rig.act("resumeRun");
    let sent = rig.run_to_quiet();
    assert!(!sent.iter().any(|line| line.starts_with("G1 Z-3")), "{sent:?}");
}

#[test]
fn a_jog_never_releases_job_lines_the_firmware_holds_behind_an_unexpected_pause() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&["G1 X1", "G1 Z-3", "G1 X2", "G1 X3"]));
    rig.act("startRun");
    rig.rx("Message: Please adjust Z-Axis to a depth of -3.00 mm");
    rig.rx("Maslow Paused");
    let effects = rig.act_with("move", json!("right"), json!("10"));
    assert!(written(&effects).is_empty());
    assert!(alerts(&effects)[0].starts_with("The controller paused with lines of the job still in its buffer"));
    assert!(rig.run_to_quiet().iter().all(|line| line != "~"));
}

#[test]
fn resume_releases_held_job_lines_and_lets_jogs_release_the_next_pause() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&["G1 X1", "G1 Z-3", "G1 X2"]));
    rig.act("startRun");
    rig.rx("Message: Please adjust Z-Axis to a depth of -3.00 mm");
    rig.rx("Maslow Paused");
    assert_eq!(written(&rig.act("resumeRun")), ["~"]);
    assert_eq!(rig.machine.snapshot().upload_flag, 1, "manual Z carries on with the line it paused in");
    rig.run_to_quiet();
    rig.send(Event::Request { id: 1, request: Request::SendGcode("M0".into()) });
    rig.run_to_quiet();
    rig.rx("Maslow Paused");
    assert_eq!(written(&rig.act_with("move", json!("right"), json!("10"))), ["~"]);
}

#[test]
fn outside_a_job_a_jog_does_not_confirm_a_manual_z_adjustment() {
    let mut rig = Rig::connected();
    rig.rx("Message: Please adjust Z-Axis to a depth of -3.00 mm");
    rig.rx("Maslow Paused");
    let effects = rig.act_with("move", json!("right"), json!("10"));
    assert!(written(&effects).is_empty());
    assert_eq!(
        alerts(&effects),
        ["The controller is waiting for Z to be adjusted by hand. Adjust it, then press Resume."]
    );
    assert_eq!(written(&rig.act("resumeRun")), ["~"]);
    assert_eq!(rig.machine.snapshot().upload_flag, 0);
}

#[test]
fn resume_after_a_manual_z_prompt_carries_on_without_moving() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&["G1 X1", "G1 Z-3", "G1 X2"]));
    rig.act("startRun");
    rig.advance(60);
    rig.rx("ok");
    rig.rx("Message: Please adjust Z-Axis to a depth of -3.00 mm");
    rig.rx("Maslow Paused");
    rig.clear_written();
    assert_eq!(written(&rig.act("resumeRun")), ["~"]);
    let snapshot = rig.machine.snapshot();
    assert_eq!((snapshot.upload_flag, snapshot.gcode_index, snapshot.manual_z_axis_adjust), (1, 2, false));
}

#[test]
fn a_reconnect_or_a_controller_boot_forgets_the_pause_and_the_tool() {
    let mut rig = Rig::connected();
    stream(&mut rig, &["T4 M6", "G1 X2"]);
    tool_change(&mut rig, 4);
    rig.rx("Grbl v1.00");
    let snapshot = rig.machine.snapshot();
    assert_eq!(snapshot.current_tool, 0);
    assert!(written(&rig.act_with("move", json!("right"), json!("10"))).iter().all(|line| line != "~"));
    let mut reopened = Rig::connected();
    stream(&mut reopened, &["T4 M6", "G1 X2"]);
    tool_change(&mut reopened, 4);
    reopened.open();
    assert_eq!(reopened.machine.snapshot().current_tool, 0);
}

#[test]
fn the_firmwares_tool_change_message_corrects_the_tracked_tool() {
    let mut rig = Rig::connected();
    stream(&mut rig, &["T2 M6", "G1 X2"]);
    tool_change(&mut rig, 5);
    assert_eq!(rig.machine.snapshot().current_tool, 5);
}

#[test]
fn an_m0_or_m6_outside_a_job_does_not_hold_streaming() {
    let mut rig = Rig::connected();
    rig.send(Event::Request { id: 1, request: Request::SendGcode("M0\nT2 M6".into()) });
    rig.run_to_quiet();
    assert_eq!(rig.machine.snapshot().upload_flag, 0);
}

#[test]
fn the_t_added_to_an_m6_can_push_it_over_the_controllers_limit() {
    let mut rig = Rig::connected();
    let long_m6 = format!("M6{}", " X1".repeat(40));
    let sent = stream(&mut rig, &["T2", &long_m6, "G1 X2"]);
    assert!(sent.is_empty());
    assert_eq!(job_state(&rig), (2, 0, 1));
}

#[test]
fn a_restart_changes_to_the_tool_the_file_last_changed_to() {
    let restarts: [(&[&str], &str); 4] = [
        (&["G21", "T2", "M6", "G0 X1"], "T2 M6"),
        (&["G21", "M6 T3", "G0 X1"], "T3 M6"),
        (&["G21", "T2", "M6", "T5", "G0 X1"], "T2 M6"),
        (&["G21", "G0 X1 T4", "M6", "G0 X1"], "T4 M6"),
    ];
    restarts.iter().for_each(|(lines, change)| {
        let mut rig = Rig::connected();
        rig.machine.load_gcode(&file(lines));
        rig.act_with("moveGcodeGoto", json!(lines.len() - 1), json!(""));
        rig.clear_written();
        rig.act("startRun");
        let sent = rig.run_to_quiet();
        assert_eq!(sent.iter().filter(|line| line.contains("M6")).collect::<Vec<_>>(), [change], "{lines:?}");
    });
}

#[test]
fn a_tool_change_line_never_overfills_the_controller_buffer() {
    let mut rig = Rig::connected_with(rig::settings(&[("Maslow Settings", "bufferOn", json!(1))]));
    rig.open();
    ["PCB v1.6 TLE9201  Detected", "Grbl v1.00", "ready", "ok"].iter().for_each(|line| {
        rig.rx(line);
    });
    rig.run_to_quiet();
    let mut lines = vec!["T2"];
    lines.extend(["G1 X1 Y1"; 10]);
    lines.extend(["X1", "M6", "G1 X2"]);
    rig.machine.load_gcode(&file(&lines));
    rig.clear_written();
    rig.act("startRun");
    let sent: Vec<String> = written(&rig.advance(2000));
    assert!(sent.iter().map(|line| line.len() + 2).sum::<usize>() <= 126, "{sent:?}");
    assert!(sent.len() > 5, "buffered: lines go out without acks: {sent:?}");
}

#[test]
fn the_tool_change_prompt_names_the_tool_and_the_file_says_what_it_is() {
    let mut rig = Rig::connected();
    stream(&mut rig, &["(T2 D=3.175 CR=0 - ZMIN=-1 - flat end mill)", "G21", "T2 M6", "G1 X2"]);
    let prompt = pushed(&rig.rx("Tool Change: Please insert tool 2"), "activateModal");
    assert_eq!(prompt[0]["message"], "Please insert tool 2");
    assert_eq!((prompt[0]["tool"].clone(), prompt[0]["resume"].clone()), (json!(2), json!("resume")));
    let tools = rig.machine.snapshot().file_tools;
    assert_eq!(tools[0].description.as_deref(), Some("3.175 mm (1/8\") flat end mill"));
}

#[test]
fn a_prompt_without_a_number_names_the_tool_the_file_changes_to() {
    let mut rig = Rig::connected();
    stream(&mut rig, &["G1 X1", "T4 M6", "G1 X2"]);
    let prompt = pushed(&rig.rx("Tool Change: Please insert a tool"), "activateModal");
    assert_eq!(prompt[0]["tool"], 4);
    let mut fresh = Rig::connected();
    let prompt = pushed(&fresh.rx("Tool Change: Please insert a tool"), "activateModal");
    assert!(prompt[0].get("tool").is_none());
}
