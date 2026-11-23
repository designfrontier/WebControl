//! The Z panel's Go to Z0 and Go to Safe Height: what they send, and the gates they share with Raise and Lower.

mod rig;

use rig::{Rig, alerts, settings, written};
use serde_json::json;

const NO_ZERO: &str = "Z zero has not been set since the controller connected";

fn zeroed(changes: &[(&str, &str, serde_json::Value)]) -> Rig {
    let mut rig = Rig::connected_with(settings(changes));
    rig.act("defineZ0");
    rig.run_to_quiet();
    rig.clear_written();
    rig
}

fn sent_by(rig: &mut Rig, command: &str) -> Vec<String> {
    rig.act(command);
    rig.run_to_quiet()
}

#[test]
fn go_to_z0_sends_the_units_g90_and_a_rapid_to_z0() {
    assert_eq!(sent_by(&mut zeroed(&[]), "zToZero"), ["G21", "G90", "G00 Z0"]);
    let inches = [("Computed Settings", "units", json!("INCHES"))];
    assert_eq!(sent_by(&mut zeroed(&inches), "zToZero"), ["G20", "G90", "G00 Z0"]);
}

#[test]
fn go_to_z0_is_refused_until_z_zero_is_set() {
    let mut rig = Rig::connected();
    let effects = rig.act("zToZero");
    assert!(alerts(&effects)[0].starts_with(NO_ZERO), "{:?}", alerts(&effects));
    assert!(rig.run_to_quiet().is_empty());
}

#[test]
fn a_touch_sets_z_zero_and_a_controller_restart_forgets_it() {
    let mut rig = Rig::connected();
    rig.act("touchZ");
    rig.advance(100);
    rig.rx("z axis zeroed");
    rig.rx("ok");
    rig.clear_written();
    assert_eq!(sent_by(&mut rig, "zToZero"), ["G21", "G90", "G00 Z0"]);
    ["PCB v1.6 TLE9201  Detected", "Grbl v1.00", "ready", "ok"].iter().for_each(|line| {
        rig.rx(line);
    });
    rig.run_to_quiet();
    assert!(alerts(&rig.act("zToZero"))[0].starts_with(NO_ZERO));
}

#[test]
fn go_to_safe_height_rises_as_home_does_and_never_lowers_the_bit() {
    let mut rig = zeroed(&[]);
    assert_eq!(sent_by(&mut rig, "zToSafeHeight"), ["G21", "G90", "G00 Z5.0"]);
    rig.act_with("moveZ", json!("raise"), json!("20"));
    rig.run_to_quiet();
    assert_eq!(sent_by(&mut rig, "zToSafeHeight"), ["G21", "G90", "G00 Z25.0"]);
    let inches = [("Computed Settings", "units", json!("INCHES"))];
    assert_eq!(sent_by(&mut zeroed(&inches), "zToSafeHeight"), ["G20", "G90", "G00 Z0.197"]);
}

#[test]
fn go_to_safe_height_needs_a_safe_height_above_zero() {
    let mut rig = zeroed(&[("Maslow Settings", "zAxisSafeHeight", json!("0"))]);
    let effects = rig.act("zToSafeHeight");
    assert!(alerts(&effects)[0].starts_with("Set the Z-axis safe height"), "{:?}", alerts(&effects));
    assert!(rig.run_to_quiet().is_empty());
}

#[test]
fn both_are_refused_while_a_job_streams_or_a_probe_runs() {
    let mut rig = zeroed(&[]);
    rig.machine.load_gcode(&"G1 X10 F100\n".repeat(50));
    rig.act("startRun");
    ["zToZero", "zToSafeHeight"].iter().for_each(|command| {
        assert_eq!(alerts(&rig.act(command)), ["Cannot issue command while sending gcode."]);
    });
    let mut probing = zeroed(&[]);
    probing.act("touchZ");
    ["zToZero", "zToSafeHeight"].iter().for_each(|command| {
        assert!(alerts(&probing.act(command))[0].starts_with("Wait for Touch Z"));
    });
}

/// Streams `file` and answers each line before its M6, which waits unanswered for the firmware's `~`.
fn paused_at_an_unanswered_tool_change(rig: &mut Rig, file: &str) {
    rig.machine.load_gcode(file);
    let mut pending = written(&rig.act("startRun"));
    let mut reached_m6 = false;
    while !pending.is_empty() || !reached_m6 {
        if pending.is_empty() {
            pending = written(&rig.advance(100));
            assert!(!pending.is_empty(), "the file never reached its M6");
        }
        let line = pending.remove(0);
        reached_m6 |= line.contains("M6");
        if !reached_m6 {
            pending.extend(written(&rig.rx("ok")));
        }
    }
    rig.rx("Tool Change: Please insert tool 2");
    rig.rx("Maslow Paused");
    rig.clear_written();
}

#[test]
fn at_a_tool_change_both_release_the_firmware_as_raise_does() {
    let mut rig = zeroed(&[]);
    paused_at_an_unanswered_tool_change(&mut rig, "G21\nG0 Z5\nT2 M6\nG1 X2\n");
    assert_eq!(written(&rig.act("zToSafeHeight")), ["~"]);
    rig.rx("ok");
    rig.rx("ok");
    assert_eq!(rig.run_to_quiet(), ["~", "G21", "G90", "G00 Z5.0"], "the M6's ok lets the raise out");
    assert_eq!(sent_by(&mut rig, "zToZero"), ["G21", "G90", "G00 Z0"]);
    assert!(rig.machine.snapshot().manual_z_axis_adjust, "the tool change still waits for Resume");
}

#[test]
fn a_numbered_tool_change_line_still_lets_raise_release_the_firmware() {
    let mut rig = zeroed(&[]);
    paused_at_an_unanswered_tool_change(&mut rig, "N1 G21\nN2 G0 Z5\nN3 T2 M6\nN4 G1 X2\n");
    assert_eq!(written(&rig.act_with("moveZ", json!("raise"), json!("10")))[0], "~");
}

#[test]
fn no_tilde_while_the_tool_change_line_moves_on_after_its_m6() {
    let mut rig = zeroed(&[]);
    paused_at_an_unanswered_tool_change(&mut rig, "G21\nG0 Z5\nT2 M6 G0 X40 Y40\nG1 Z-3 F100\n");
    let held = "The controller paused with other lines waiting behind the pause";
    ["zToSafeHeight", "zToZero"].iter().for_each(|command| {
        let effects = rig.act(command);
        assert!(written(&effects).is_empty(), "{command}: {:?}", written(&effects));
        assert!(alerts(&effects)[0].starts_with(held), "{command}: {:?}", alerts(&effects));
    });
    let raise = rig.act_with("moveZ", json!("raise"), json!("10"));
    assert!(written(&raise).is_empty() && alerts(&raise)[0].starts_with(held));
    assert!(rig.advance(100).is_empty() && rig.machine.snapshot().manual_z_axis_adjust);
}

#[test]
fn no_tilde_while_a_jog_waits_behind_the_tool_change() {
    let mut rig = zeroed(&[]);
    rig.machine.load_gcode("G21\nG0 Z5\nT2 M6\nG1 X2\n");
    rig.act("startRun");
    [0, 1].iter().for_each(|_| {
        rig.advance(100);
        rig.rx("ok");
    });
    rig.advance(100);
    rig.act_with("move", json!("right"), json!("10"));
    rig.rx("Tool Change: Please insert tool 2");
    rig.rx("Maslow Paused");
    rig.clear_written();
    let effects = rig.act("zToSafeHeight");
    assert!(written(&effects).is_empty(), "the ~ would run the jog first: {:?}", written(&effects));
    assert!(alerts(&effects)[0].starts_with("The controller paused with other lines waiting"));
}

#[test]
fn go_to_z0_refused_at_a_tool_change_sends_no_tilde() {
    let mut rig = Rig::connected();
    paused_at_an_unanswered_tool_change(&mut rig, "G21\nT2 M6\nG1 X2\n");
    let effects = rig.act("zToZero");
    assert!(alerts(&effects)[0].starts_with(NO_ZERO), "{:?}", alerts(&effects));
    assert!(written(&effects).is_empty() && rig.advance(100).is_empty(), "the firmware stays paused");
}

#[test]
fn a_refused_safe_height_at_a_tool_change_sends_no_tilde() {
    let mut rig = zeroed(&[("Maslow Settings", "zAxisSafeHeight", json!("0"))]);
    paused_at_an_unanswered_tool_change(&mut rig, "G21\nT2 M6\nG1 X2\n");
    let effects = rig.act("zToSafeHeight");
    assert!(alerts(&effects)[0].starts_with("Set the Z-axis safe height"), "{:?}", alerts(&effects));
    assert!(written(&effects).is_empty() && rig.advance(100).is_empty(), "the firmware stays paused");
}
