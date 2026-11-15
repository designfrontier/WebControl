//! The setup screens' maintenance actions, byte for byte as Actions/actions.py queues them, the B10 measurements
//! and the PID test's data lines (messageProcessor).

mod rig;

use rig::{Rig, alerts, pushed, settings, written};
use serde_json::{Value, json};

/// Lets `ms` pass with the controller reporting its status every 200 ms, as it does; returns what was written.
fn idle(rig: &mut Rig, ms: u64) -> Vec<String> {
    (0..ms / 200)
        .flat_map(|_| {
            let mut effects = rig.advance(200);
            effects.extend(rig.rx("<Idle,MPos:0.00,0.00,0.00,WPos:0.000,0.000,0.000>"));
            written(&effects)
        })
        .collect()
}

/// Every line the action writes, each answered `ok`, and its alerts.
fn sent(rig: &mut Rig, command: &str, arg: Value, arg1: Value) -> (Vec<String>, Vec<String>) {
    let effects = rig.act_with(command, arg, arg1);
    (rig.run_to_quiet(), alerts(&effects))
}

#[test]
fn simple_commands_go_out_as_python_sends_them() {
    let cases = [
        ("testMotors", json!(""), json!(""), vec!["B04"]),
        ("setSprocketsZero", json!(""), json!(""), vec!["B06 L0 R0"]),
        ("setSprocketsDefault", json!(""), json!(""), vec!["B08"]),
        ("resetChainLengths", json!(""), json!(""), vec!["B08", "G21"]),
        ("setFakeServo", json!(1), json!(""), vec!["B99 ON"]),
        ("setFakeServo", json!(0), json!(""), vec!["B99 OFF"]),
        ("rotateSprocket", json!("L"), json!(5), vec!["B11 L S100 T5"]),
        ("rotateSprocket", json!("R"), json!(-2.5), vec!["B11 R S-100 T2.5"]),
        ("rotateSprocket", json!("R"), json!(-3), vec!["B11 R S-100 T3"]),
        ("rotateSprocket", json!("L"), json!(1.5), vec!["B11 L S100 T1.5"]),
    ];
    for (command, arg, arg1, expected) in cases {
        let mut rig = Rig::connected();
        let (lines, refused) = sent(&mut rig, command, arg.clone(), arg1.clone());
        assert_eq!(lines, expected, "{command} {arg} {arg1}");
        assert!(refused.is_empty(), "{command}: {refused:?}");
    }
}

#[test]
fn reset_chain_lengths_sends_the_machines_units() {
    let mut rig = Rig::connected_with(settings(&[("Computed Settings", "units", json!("INCHES"))]));
    assert_eq!(sent(&mut rig, "resetChainLengths", json!(""), json!("")).0, ["B08", "G20"]);
}

#[test]
fn set_sprockets_turns_by_chain_length_and_reverses_under_the_sprocket() {
    let mut rig = Rig::connected();
    assert_eq!(sent(&mut rig, "setSprockets", json!("L"), json!(90)).0, ["G91", "B09 L-15.875", "G90"]);
    let mut top = Rig::connected_with(settings(&[
        ("Advanced Settings", "chainOverSprocket", json!("Top")),
        ("Computed Settings", "chainOverSprocketComputed", json!(1)),
    ]));
    assert_eq!(sent(&mut top, "setSprockets", json!("R"), json!(-1)).0, ["G91", "B09 R-0.1764", "G90"]);
    let mut zero = Rig::connected();
    assert_eq!(sent(&mut zero, "setSprockets", json!("L"), json!(0)).0, ["G91", "B09 L-0.0", "G90"]);
}

#[test]
fn set_sprockets_warns_when_the_computed_side_disagrees() {
    let mut rig = Rig::connected_with(settings(&[("Computed Settings", "chainOverSprocketComputed", json!("2"))]));
    let (lines, warned) = sent(&mut rig, "setSprockets", json!("L"), json!(90));
    assert_eq!(lines, ["G91", "B09 L-15.875", "G90"]);
    assert_eq!(warned.len(), 1);
    assert!(warned[0].contains("set for bottom feed, but computed !=2"));
    let mut top = Rig::connected_with(settings(&[("Advanced Settings", "chainOverSprocket", json!("Top"))]));
    assert!(sent(&mut top, "setSprockets", json!("L"), json!(90)).1[0].contains("set for top feed"));
}

#[test]
fn malformed_sprocket_arguments_alert_and_send_nothing() {
    let cases = [
        ("setSprockets", json!("L"), json!("90"), "Error with setting sprocket"),
        ("setSprockets", json!(1), json!(90), "Error with setting sprocket"),
        ("rotateSprocket", json!("L"), json!("5"), "Error with setting sprocket"),
        ("rotateSprocket", json!(5), json!(5), "Error with setting sprocket"),
        ("wipeEEPROM", json!("Most"), json!(""), "Error with wiping EEPROM"),
        ("adjustChain", json!(1), json!(""), "Error with adjusting chain."),
    ];
    for (command, arg, arg1, alert) in cases {
        let mut rig = Rig::connected();
        let (lines, refused) = sent(&mut rig, command, arg.clone(), arg1);
        assert!(lines.is_empty(), "{command} {arg}: {lines:?}");
        assert_eq!(refused, [alert], "{command} {arg}");
    }
    let mut broken = Rig::connected_with(settings(&[("Advanced Settings", "gearTeeth", json!("many"))]));
    assert_eq!(sent(&mut broken, "setSprockets", json!("L"), json!(90)).1, ["Error with setting sprocket"]);
}

#[test]
fn setup_actions_need_the_controller() {
    let mut rig = Rig::new();
    ["testMotors", "executeVelocityPIDTest"].iter().for_each(|command| {
        assert_eq!(alerts(&rig.act(command)), ["The controller is not connected."], "{command}");
    });
}

#[test]
fn automatic_sprockets_measure_both_chains_then_turn_back_to_vertical() {
    let mut rig = Rig::connected();
    assert_eq!(sent(&mut rig, "setSprocketAutomatic", json!(""), json!("")).0, ["B10 L"]);
    rig.rx("[Measure: 1651.25]");
    assert_eq!(rig.run_to_quiet(), ["B10 R"]);
    rig.rx("[Measure: 1600.0]");
    assert_eq!(rig.run_to_quiet(), ["G91", "B09 L-0.25", "B09 R-12.5", "G90"]);
    rig.rx("[Measure: 1600.0]");
    assert!(rig.run_to_quiet().is_empty(), "a measurement nobody asked for moves nothing");
}

#[test]
fn turning_back_needs_a_readable_sprocket() {
    let mut rig = Rig::connected_with(settings(&[("Advanced Settings", "chainPitch", json!("0"))]));
    rig.act("setSprocketAutomatic");
    rig.run_to_quiet();
    rig.rx("[Measure: 10]");
    rig.run_to_quiet();
    rig.rx("[Measure: 10]");
    assert!(rig.run_to_quiet().is_empty());
}

#[test]
fn adjust_chain_counts_down_then_extends_until_measured_then_stops() {
    let mut rig = Rig::connected();
    let effects = rig.act_with("adjustChain", json!("left"), json!(""));
    assert!(written(&effects).is_empty());
    assert!(idle(&mut rig, 5800).is_empty(), "the countdown holds the queue");
    assert_eq!(idle(&mut rig, 400), ["B02 L1 R0"]);
    assert_eq!(written(&rig.rx("ok")), ["B10 L"]);
    rig.rx("ok");
    let stop = rig.rx("[Measure: 1651.0]");
    assert_eq!(written(&stop), ["!"]);
    let mut right = Rig::connected();
    right.act_with("adjustChain", json!("right"), json!(""));
    assert_eq!(idle(&mut right, 6200), ["B02 L0 R1"]);
    let mut neither = Rig::connected();
    assert!(alerts(&neither.act_with("adjustChain", json!("up"), json!(""))).is_empty());
    assert!(idle(&mut neither, 7000).is_empty());
}

#[test]
fn wiping_the_eeprom_waits_before_reading_the_settings_back() {
    for (extent, reset, chains) in [("All", "$RST=*", true), ("Settings", "$RST=$", false), ("Maslow", "$RST=#", true)]
    {
        let mut rig = Rig::connected();
        let effects = rig.act_with("wipeEEPROM", json!(extent), json!(""));
        assert_eq!(written(&effects), [reset], "the reset gets no ok of its own");
        assert!(written(&rig.advance(1950)).is_empty(), "{extent}");
        assert_eq!(written(&rig.advance(100)), ["$$"]);
        let mut all = vec![reset, "$$"];
        all.extend(if chains { vec!["B08", "G21"] } else { Vec::new() });
        assert_eq!(rig.run_to_quiet(), all, "{extent}");
    }
}

#[test]
fn stop_drops_a_pending_wait_and_what_follows_it() {
    let mut rig = Rig::connected();
    rig.act_with("adjustChain", json!("left"), json!(""));
    rig.act("stopRun");
    assert!(rig.run_to_quiet().iter().all(|line| line != "B02 L1 R0"));
    rig.rx("<Idle,MPos:0.00,0.00,0.00,WPos:0.000,0.000,0.000>");
    assert!(written(&rig.rx("[Measure: 10]")).is_empty());
}

fn velocity(version: &str) -> Value {
    json!({"vMotor": "L", "vStart": "0", "vStop": "1", "vSteps": "2", "vVersion": version, "KpV": "5", "KiV": "0",
        "KdV": "0.3"})
}

#[test]
fn a_velocity_test_collects_numbers_until_its_stop_marker_then_stops() {
    let mut rig = Rig::connected();
    assert_eq!(sent(&mut rig, "executeVelocityPIDTest", velocity("1"), json!("")).0, ["B13 L1 S0 F1 I2 V1"]);
    let lines = ["--PID Velocity Test Start--", "Kp=5", "12.5", "nope", " 13 "];
    let quiet: Vec<_> = lines.iter().flat_map(|line| rig.rx(line)).collect();
    assert!(pushed(&quiet, "controllerMessage").is_empty(), "test lines are not shown");
    let done = rig.rx("--PID Velocity Test Stop--");
    assert_eq!(
        pushed(&done, "updatePIDData"),
        [json!({"command": "updatePIDData", "data": {"result": "velocity",
        "version": "1", "data": [12.5, 13.0]}})]
    );
    assert_eq!(written(&done), ["!"]);
    assert!(!pushed(&done, "clearAlarm").is_empty());
}

#[test]
fn a_version_2_position_test_keeps_its_lines_as_text() {
    let mut rig = Rig::connected();
    let position = json!({"pMotor": "R", "pStart": "0", "pStop": "5", "pSteps": "1", "pTime": "2", "pVersion": "2",
        "KpP": "1300", "KiP": "0", "KdP": "34"});
    assert_eq!(sent(&mut rig, "executePositionPIDTest", position, json!("")).0, ["B14 R1 S0 F5 I1 T2 V2"]);
    ["--PID Position Test Start--", "setpoint,input", "1.0,0.9,30", "error: bad"].iter().for_each(|line| {
        rig.rx(line);
    });
    let done = rig.rx("--PID Position Test Stop--");
    assert_eq!(pushed(&done, "updatePIDData")[0]["data"]["data"], json!(["1.0,0.9,30", "error: bad"]));
}

#[test]
fn a_stop_marker_without_a_test_and_stop_itself_end_nothing_or_everything() {
    let mut rig = Rig::connected();
    let stray = rig.rx("--PID Velocity Test Stop--");
    assert!(pushed(&stray, "updatePIDData").is_empty());
    assert!(written(&stray).is_empty());
    rig.rx("--PID Position Test Start--");
    rig.act("stopRun");
    let after = rig.rx("42");
    assert_eq!(pushed(&after, "controllerMessage"), [json!("42")]);
    let unrequested = rig.rx("--PID Velocity Test Start--");
    assert!(pushed(&unrequested, "controllerMessage").is_empty());
    rig.rx("7");
    let reported = rig.rx("--PID Velocity Test Stop--");
    assert_eq!(pushed(&reported, "updatePIDData")[0]["data"]["version"], 0);
}

#[test]
fn a_malformed_pid_action_alerts() {
    let mut rig = Rig::connected();
    let mut missing = velocity("1");
    missing.as_object_mut().unwrap().remove("vSteps");
    let (lines, refused) = sent(&mut rig, "executeVelocityPIDTest", missing, json!(""));
    assert!(lines.is_empty());
    assert_eq!(refused, ["Error with executing velocity PID test."]);
    assert_eq!(
        sent(&mut rig, "executePositionPIDTest", json!(""), json!("")).1,
        ["Error with executing velocity PID test."]
    );
}
