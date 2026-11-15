//! Owner decision (2026-11-15): nothing travels in XY after a resume or restart until Z is at the higher of the safe
//! height and the bit's own height, and no Z move before that travel lowers the bit.

mod rig;

use rig::{Rig, alerts, settings, written};
use serde_json::json;
use wc_machine::Event;
use wc_machine::files::Request;
use wc_protocol::main::PauseButton;

/// `wc_machine`'s wait for a status report when the bit's height is unknown.
const HEIGHT_WAIT_MS: u64 = 3000;

const JOB: [&str; 7] = ["G21", "G90", "G0 Z5", "G0 X10 Y10", "G1 Z-1 F100", "G1 X20", "G1 X30"];

fn file(lines: &[&str]) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
}

fn pause_at(rig: &mut Rig, lines: &[&str], index: usize) {
    rig.machine.load_gcode(&file(lines));
    rig.act("startRun");
    while rig.machine.snapshot().gcode_index < index {
        rig.advance(60);
        rig.rx("ok");
    }
    rig.act("pauseRun");
    rig.rx("ok");
    rig.advance(100);
}

fn move_z(rig: &mut Rig, direction: &str, mm: &str) {
    rig.act_with("moveZ", json!(direction), json!(mm));
    rig.run_to_quiet();
}

fn resume(rig: &mut Rig) -> Vec<String> {
    rig.clear_written();
    rig.act("resumeRun");
    rig.run_to_quiet()
}

/// The Z moves sent before the first XY move.
fn before_travel(sent: &[String]) -> Vec<String> {
    sent.iter().take_while(|line| !line.contains('X')).filter(|line| line.contains('Z')).cloned().collect()
}

#[test]
fn resume_after_a_pause_travels_at_the_bits_height_when_it_is_above_safe_height() {
    let mut rig = Rig::connected();
    pause_at(&mut rig, &JOB, 6);
    move_z(&mut rig, "raise", "11");
    let sent = resume(&mut rig);
    assert_eq!(before_travel(&sent), ["G0 Z10.0"], "{sent:?}");
    assert!(sent.contains(&"G1 Z-1.0 F100.0".to_string()));
}

#[test]
fn resume_after_a_pause_rises_to_safe_height_from_below_it() {
    let mut rig = Rig::connected();
    pause_at(&mut rig, &JOB, 6);
    move_z(&mut rig, "raise", "2");
    assert_eq!(before_travel(&resume(&mut rig)), ["G0 Z5.0"]);
}

#[test]
fn resume_after_redefining_z_zero_never_lowers_the_bit() {
    let mut rig = Rig::connected();
    let lines = ["G21", "G90", "G0 Z30", "G0 X10 Y10", "G1 Z23 F1000", "G1 Z30", "G1 X20", "G1 X30"];
    pause_at(&mut rig, &lines, 7);
    move_z(&mut rig, "lower", "10");
    rig.act("defineZ0");
    rig.run_to_quiet();
    move_z(&mut rig, "raise", "10");
    let sent = resume(&mut rig);
    assert_eq!(before_travel(&sent), ["G0 Z10.0"], "{sent:?}");
}

#[test]
fn a_raise_still_queued_when_resume_is_pressed_counts() {
    let mut rig = Rig::connected();
    pause_at(&mut rig, &JOB, 6);
    rig.clear_written();
    rig.act_with("moveZ", json!("raise"), json!("20"));
    rig.act("resumeRun");
    let sent = rig.run_to_quiet();
    assert_eq!(before_travel(&sent), ["G00 Z20.0", "G0 Z19.0"], "{sent:?}");
}

#[test]
fn play_mid_file_travels_at_the_bits_height() {
    let mut rig = Rig::connected();
    rig.act("defineZ0");
    move_z(&mut rig, "raise", "25");
    rig.machine.load_gcode(&file(&JOB));
    rig.act_with("moveGcodeGoto", json!("5"), json!(""));
    rig.clear_written();
    rig.act("startRun");
    let sent = rig.run_to_quiet();
    assert_eq!(before_travel(&sent), ["G0 Z25.0"], "{sent:?}");
}

#[test]
fn resume_after_a_tool_change_travels_at_the_new_bits_height() {
    let mut rig = Rig::connected();
    let lines = ["G21", "G90", "G0 Z5", "G0 X10 Y10", "G1 Z-3 F100", "G0 Z5", "T2 M6", "G0 X40 Y40", "G1 Z-2"];
    rig.machine.load_gcode(&file(&lines));
    rig.act("startRun");
    rig.run_to_quiet();
    rig.rx("Tool Change: Please insert tool 2");
    rig.rx("Maslow Paused");
    move_z(&mut rig, "raise", "40");
    assert_eq!(before_travel(&resume(&mut rig)), ["G0 Z45.0"]);
}

#[test]
fn resume_after_an_m0_travels_at_the_bits_height() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&["G21", "G90", "G0 Z12", "M0", "G0 X10 Y10", "G1 Z-1"]));
    rig.act("startRun");
    rig.run_to_quiet();
    rig.rx("Maslow Paused");
    assert_eq!(before_travel(&resume(&mut rig)), ["G0 Z12.0"]);
}

#[test]
fn after_an_alarm_the_reported_height_rounded_up_is_the_bits_height() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&JOB));
    rig.act("startRun");
    (0..4).for_each(|_| {
        rig.advance(60);
        rig.rx("ok");
    });
    rig.rx("ALARM: The sled is not keeping up with its expected position and has halted.");
    rig.advance(300);
    rig.rx("<Idle,MPos:10.00,10.00,8.25,WPos:0.000,0.000,0.000>");
    rig.advance(100);
    rig.clear_written();
    rig.act("startRun");
    let sent = rig.run_to_quiet();
    assert_eq!(before_travel(&sent), ["G0 Z8.26"], "{sent:?}");
}

#[test]
fn a_report_in_inches_is_rounded_up_by_its_own_resolution() {
    let mut rig = Rig::connected_with(settings(&[("Computed Settings", "units", json!("INCHES"))]));
    rig.machine.load_gcode(&file(&["G20", "G90", "G0 Z0.2", "G0 X1 Y1", "G1 Z-0.1", "G1 X2"]));
    rig.act("startRun");
    (0..3).for_each(|_| {
        rig.advance(60);
        rig.rx("ok");
    });
    rig.act("stopRun");
    rig.advance(300);
    rig.rx("<Idle,MPos:1.00,1.00,0.50,WPos:0.000,0.000,0.000>");
    rig.advance(100);
    rig.act_with("moveGcodeGoto", json!("5"), json!(""));
    rig.clear_written();
    rig.act("startRun");
    assert_eq!(before_travel(&rig.run_to_quiet()), ["G0 Z0.51"]);
}

#[test]
fn a_jog_from_an_unknown_height_adds_to_the_reported_one_rounded_up() {
    let mut rig = Rig::connected();
    rig.rx("<Idle,MPos:0.00,0.00,3.00,WPos:0.000,0.000,0.000>");
    move_z(&mut rig, "raise", "4");
    rig.act("returnToCenter");
    assert_eq!(rig.run_to_quiet(), ["G90", "G00 Z7.01", "G00 X0.0 Y0.0"]);
}

fn booted_without_a_report() -> Rig {
    let mut rig = Rig::with(settings(&[]));
    rig.open();
    ["PCB v1.1 Detected", "Grbl v1.00", "ready", "ok"].iter().for_each(|line| {
        rig.rx(line);
    });
    rig.run_to_quiet();
    rig
}

const REPORT_AT_10: &str = "<Idle,MPos:0.00,0.00,10.00,WPos:0.000,0.000,0.000>";
const REPORT_AT_20: &str = "<Idle,MPos:0.00,0.00,20.00,WPos:0.000,0.000,0.000>";

#[test]
fn with_no_height_known_the_raise_waits_for_a_fresh_report() {
    let mut rig = booted_without_a_report();
    rig.act_with("moveZ", json!("raise"), json!("10"));
    rig.act("returnToCenter");
    let sent = rig.run_to_quiet();
    assert_eq!(sent[4..], ["G90"], "{sent:?}");
    assert!(written(&rig.advance(2000)).is_empty(), "no guess at the safe height");
    rig.rx(REPORT_AT_10);
    assert_eq!(rig.run_to_quiet(), ["G00 Z10.01", "G00 X0.0 Y0.0"]);
}

#[test]
fn with_no_report_in_time_the_move_is_refused() {
    let mut rig = booted_without_a_report();
    rig.act("returnToCenter");
    rig.run_to_quiet();
    let refused = rig.advance(HEIGHT_WAIT_MS + 50);
    assert!(written(&refused).is_empty());
    assert!(alerts(&refused).iter().any(|alert| alert.contains("could not tell how high the bit is")), "{refused:?}");
    rig.rx(REPORT_AT_10);
    assert!(rig.run_to_quiet().is_empty(), "the refused travel never goes out");
}

#[test]
fn stop_drops_a_waiting_raise_at_once() {
    let mut rig = booted_without_a_report();
    rig.act("returnToCenter");
    rig.run_to_quiet();
    let stop = rig.act("stopRun");
    assert_eq!(written(&stop), ["!"]);
    rig.clear_written();
    rig.advance(300);
    rig.rx(REPORT_AT_10);
    let after = rig.run_to_quiet();
    assert!(after.is_empty(), "{after:?}");
    assert!(alerts(&rig.advance(HEIGHT_WAIT_MS + 50)).is_empty(), "no refusal for a raise Stop dropped");
}

#[test]
fn after_a_stop_mid_rise_the_raise_uses_the_report_that_ends_the_resync() {
    let mut rig = Rig::connected();
    rig.act_with("moveZ", json!("raise"), json!("20"));
    rig.advance(100);
    rig.act("stopRun");
    rig.act("home");
    rig.clear_written();
    rig.advance(300);
    rig.rx("<Idle,MPos:0.00,0.00,7.43,WPos:0.000,0.000,0.000>");
    assert_eq!(rig.run_to_quiet(), ["G90", "G00 Z7.44", "G00 X0.0 Y0.0", "G00 Z0"]);
}

#[test]
fn play_pressed_during_an_alarms_resync_waits_for_its_report() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&JOB));
    rig.act("startRun");
    (0..4).for_each(|_| {
        rig.advance(60);
        rig.rx("ok");
    });
    rig.rx("ALARM: The sled is not keeping up with its expected position and has halted.");
    rig.clear_written();
    rig.act("startRun");
    assert!(written(&rig.advance(300)).is_empty());
    rig.rx("<Idle,MPos:10.00,10.00,6.50,WPos:0.000,0.000,0.000>");
    assert_eq!(before_travel(&rig.run_to_quiet()), ["G0 Z6.51"]);
}

#[test]
fn resume_after_a_reconnect_waits_for_a_report_and_is_refused_without_one() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&JOB));
    rig.act("startRun");
    (0..5).for_each(|_| {
        rig.advance(60);
        rig.rx("ok");
    });
    rig.advance(5100);
    rig.open();
    ["PCB v1.6 TLE9201  Detected", "Grbl v1.00", "ready", "ok"].iter().for_each(|line| {
        rig.rx(line);
    });
    rig.run_to_quiet();
    rig.clear_written();
    rig.act("resumeRun");
    assert_eq!(rig.run_to_quiet(), ["~"]);
    let refused = rig.advance(HEIGHT_WAIT_MS + 50);
    assert_eq!(alerts(&refused).len(), 1, "{refused:?}");
    let snapshot = rig.machine.snapshot();
    assert_eq!((snapshot.upload_flag, snapshot.pause_button), (2, PauseButton::Resume));
    rig.rx("<Idle,MPos:10.00,10.00,12.00,WPos:0.000,0.000,0.000>");
    assert!(rig.run_to_quiet().is_empty(), "nothing moves until Resume again");
    rig.act("resumeRun");
    let sent = rig.run_to_quiet();
    assert_eq!(before_travel(&sent), ["G0 Z12.01"], "{sent:?}");
}

#[test]
fn a_refused_play_mid_file_leaves_the_job_stopped_at_its_line() {
    let mut rig = booted_without_a_report();
    rig.machine.load_gcode(&file(&JOB));
    rig.act_with("moveGcodeGoto", json!("5"), json!(""));
    rig.act("startRun");
    rig.run_to_quiet();
    assert_eq!(alerts(&rig.advance(HEIGHT_WAIT_MS + 50)).len(), 1);
    let snapshot = rig.machine.snapshot();
    assert_eq!((snapshot.upload_flag, snapshot.job_in_progress, snapshot.gcode_index), (0, false, 5));
}

#[test]
fn home_and_center_never_lower_the_bit_before_travelling() {
    let mut rig = Rig::connected_with(settings(&[("Advanced Settings", "homeX", json!("12.5"))]));
    rig.act("defineZ0");
    move_z(&mut rig, "raise", "8");
    rig.act("home");
    assert_eq!(rig.run_to_quiet(), ["G90", "G00 Z8.0", "G00 X12.5 Y0.0", "G00 Z0"]);
    rig.act("returnToCenter");
    assert_eq!(rig.run_to_quiet(), ["G90", "G00 Z5.0", "G00 X0.0 Y0.0"], "home left the bit at Z0");
    move_z(&mut rig, "raise", "7.7");
    rig.act("returnToCenter");
    assert_eq!(rig.run_to_quiet(), ["G90", "G00 Z12.7", "G00 X0.0 Y0.0"]);
}

#[test]
fn center_in_inches_never_lowers_the_bit() {
    let mut rig = Rig::connected_with(settings(&[
        ("Computed Settings", "units", json!("INCHES")),
        ("Computed Settings", "unitsZ", json!("INCHES")),
    ]));
    rig.act("defineZ0");
    move_z(&mut rig, "raise", "1");
    rig.act("returnToCenter");
    assert_eq!(rig.run_to_quiet(), ["G90", "G00 Z1.000", "G00 X0.0 Y0.0"]);
    rig.act("defineZ0");
    move_z(&mut rig, "raise", "0.3931");
    rig.act("returnToCenter");
    assert_eq!(rig.run_to_quiet(), ["G90", "G00 Z0.394", "G00 X0.0 Y0.0"], "rounded up, never down");
}

#[test]
fn on_51_29x_a_resumed_spindle_gets_the_files_speed() {
    let lines = ["G21", "G90", "M3 S12000", "G0 Z5", "G0 X10 Y10", "G1 Z-1 F100", "G1 X20", "G1 X30"];
    let spindle = |version: &str| {
        let mut rig = Rig::connected_on(settings(&[]), version);
        pause_at(&mut rig, &lines, 7);
        resume(&mut rig).into_iter().filter(|line| line.starts_with(['M', 'S'])).collect::<Vec<_>>()
    };
    assert_eq!(spindle("51.29"), ["M3", "S12000.0"]);
    assert_eq!(spindle("51.27"), ["M3"]);
    assert_eq!(spindle("51.3"), ["M3"]);
}

#[test]
fn a_report_read_before_the_handshake_sets_the_units_is_ignored() {
    let mut rig = Rig::with(settings(&[("Computed Settings", "units", json!("INCHES"))]));
    rig.open();
    ["PCB v1.1 Detected", "Grbl v1.00", "ready"].iter().for_each(|line| {
        rig.rx(line);
    });
    rig.rx("<Idle,MPos:0.00,0.00,3.00,WPos:0.000,0.000,0.000>");
    rig.rx("ok");
    rig.run_to_quiet();
    rig.rx("<Idle,MPos:0.00,0.00,0.12,WPos:0.000,0.000,0.000>");
    rig.act("returnToCenter");
    assert_eq!(rig.run_to_quiet(), ["G90", "G00 Z0.197", "G00 X0.0 Y0.0"], "0.13 in is below the safe height");
}

#[test]
fn a_report_read_while_a_z_move_runs_is_not_the_bits_height() {
    let mut rig = booted_without_a_report();
    rig.act_with("moveZ", json!("raise"), json!("20"));
    rig.advance(100);
    rig.rx("ok");
    rig.advance(100);
    rig.rx("ok");
    rig.advance(100);
    rig.rx("<Idle,MPos:0.00,0.00,12.00,WPos:0.000,0.000,0.000>");
    rig.rx("ok");
    rig.run_to_quiet();
    rig.act("returnToCenter");
    rig.run_to_quiet();
    rig.rx(REPORT_AT_20);
    assert_eq!(rig.run_to_quiet(), ["G00 Z20.01", "G00 X0.0 Y0.0"]);
}

/// A missed Touch Z leaves the height unknown; `[PE:]` lines keep the link alive without telling it.
fn lose_the_height_by_a_missed_probe(rig: &mut Rig) {
    rig.act("touchZ");
    rig.run_to_quiet();
    rig.rx("error: probe did not connect");
    (0..4).for_each(|_| {
        rig.advance(1000);
        rig.rx("[PE:0.00,0.00,127,1700.00,1700.00]");
    });
}

#[test]
fn each_wait_for_a_report_gets_its_own_time() {
    let mut rig = booted_without_a_report();
    rig.act("returnToCenter");
    rig.run_to_quiet();
    rig.rx(REPORT_AT_10);
    rig.run_to_quiet();
    lose_the_height_by_a_missed_probe(&mut rig);
    rig.act("returnToCenter");
    rig.run_to_quiet();
    assert!(alerts(&rig.advance(1000)).is_empty(), "the first wait's deadline has passed, not this one's");
    rig.rx(REPORT_AT_20);
    assert_eq!(rig.run_to_quiet(), ["G00 Z20.01", "G00 X0.0 Y0.0"]);
}

#[test]
fn a_raise_after_a_stopped_wait_gets_its_own_time() {
    let mut rig = booted_without_a_report();
    rig.act("returnToCenter");
    rig.run_to_quiet();
    rig.act("stopRun");
    rig.advance(300);
    rig.rx(REPORT_AT_10);
    rig.run_to_quiet();
    lose_the_height_by_a_missed_probe(&mut rig);
    rig.act("returnToCenter");
    rig.run_to_quiet();
    assert!(alerts(&rig.advance(1000)).is_empty(), "the stopped wait's deadline is gone");
    rig.rx(REPORT_AT_20);
    assert_eq!(rig.run_to_quiet(), ["G00 Z20.01", "G00 X0.0 Y0.0"]);
}

#[test]
fn a_report_from_before_a_line_it_cannot_follow_is_stale() {
    let mut rig = Rig::connected();
    rig.rx("<Idle,MPos:0.00,0.00,3.00,WPos:0.000,0.000,0.000>");
    rig.send(Event::Request { id: 1, request: Request::SendGcode("G43 Z30".into()) });
    rig.run_to_quiet();
    rig.act("returnToCenter");
    assert_eq!(rig.run_to_quiet(), ["G90"], "no guess from the report read before the G43");
    rig.rx("<Idle,MPos:0.00,0.00,30.00,WPos:0.000,0.000,0.000>");
    assert_eq!(rig.run_to_quiet(), ["G00 Z30.01", "G00 X0.0 Y0.0"]);
}

#[test]
fn a_resume_refused_and_pressed_again_still_lowers_to_the_paused_z() {
    let mut rig = Rig::connected();
    let lines = ["G21", "G91", "G0 Z5", "G0 X10 Y10", "G1 Z-6 F100", "G1 X10", "G1 X10"];
    pause_at(&mut rig, &lines, 6);
    rig.act("touchZ");
    rig.run_to_quiet();
    rig.act("resumeRun");
    rig.run_to_quiet();
    assert_eq!(alerts(&rig.advance(HEIGHT_WAIT_MS + 50)).len(), 1);
    rig.rx(REPORT_AT_10);
    let sent = resume(&mut rig);
    assert!(sent.contains(&"G1 Z-1.0 F100.0".to_string()), "{sent:?}");
}
