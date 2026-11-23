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

#[test]
fn at_a_tool_change_they_release_the_firmware_as_raise_does() {
    let mut rig = zeroed(&[]);
    rig.machine.load_gcode("G21\nG0 Z5\nT2 M6\nG1 X2\n");
    rig.act("startRun");
    rig.run_to_quiet();
    rig.rx("Tool Change: Please insert tool 2");
    rig.rx("Maslow Paused");
    rig.clear_written();
    assert_eq!(written(&rig.act("zToSafeHeight"))[0], "~");
    rig.run_to_quiet();
    rig.clear_written();
    assert_eq!(sent_by(&mut rig, "zToZero"), ["G21", "G90", "G00 Z0"]);
    assert!(rig.machine.snapshot().manual_z_axis_adjust, "the tool change still waits for Resume");
}
