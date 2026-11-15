//! G38.2 probes, from Touch Z or from a streamed file: what waits for them, what they report, and the run state.

mod rig;

use rig::{Rig, alerts, pushed, settings, written};
use serde_json::json;
use wc_machine::Effect;

const PROBING: &str = "Wait for Touch Z to finish or press Stop";
const NOT_ZEROED: &str = "Probe finished but Z was not zeroed";
const MISSED: &str = "The probe did not touch the plate";
const PROBE_FILE: [&str; 5] = ["G21", "G0 Z5", "G38.2 Z-10 F100", "G1 Z-1 F200", "G1 X10"];

fn file(lines: &[&str]) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
}

fn buffered() -> Rig {
    Rig::connected_with(settings(&[("Maslow Settings", "bufferOn", json!(1))]))
}

/// Plays `PROBE_FILE` and acks every line before the probe; returns the lines written.
fn probing_job(rig: &mut Rig) -> Vec<String> {
    rig.machine.load_gcode(&file(&PROBE_FILE));
    rig.clear_written();
    let mut sent = written(&rig.act("startRun"));
    sent.extend(written(&rig.advance(200)));
    sent
}

fn flags(rig: &Rig) -> (i32, bool, usize, bool) {
    let snapshot = rig.machine.snapshot();
    (snapshot.upload_flag, snapshot.job_in_progress, snapshot.gcode_index, snapshot.probing)
}

#[test]
fn a_probe_in_a_file_holds_the_stream_until_it_answers() {
    let mut rig = buffered();
    assert_eq!(probing_job(&mut rig), ["G21", "G0 Z5", "G38.2 Z-10 F100"], "nothing goes out behind the probe");
    assert!(rig.machine.snapshot().probing);
    rig.rx("ok");
    rig.rx("ok");
    assert!(written(&rig.advance(200)).is_empty());
    rig.rx("probing for z axis zero");
    rig.rx("z axis zeroed");
    let touched = rig.rx("ok");
    assert!(alerts(&touched).is_empty());
    assert!(!rig.machine.snapshot().probing);
    let mut after = written(&touched);
    after.extend(written(&rig.advance(200)));
    assert_eq!(after, ["G1 Z-1 F200", "G1 X10"]);
}

#[test]
fn a_probe_in_a_file_refuses_moving_actions_but_not_harmless_ones() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&PROBE_FILE));
    rig.act("startRun");
    (0..2).for_each(|_| {
        rig.advance(100);
        rig.rx("ok");
    });
    rig.advance(100);
    assert!(rig.machine.snapshot().probing);
    rig.act("pauseRun");
    assert_eq!(alerts(&rig.act_with("move", json!("up"), json!("10"))), [PROBING]);
    assert_eq!(alerts(&rig.act("resumeRun")), [PROBING]);
    assert_eq!(alerts(&rig.act("startRun")), [PROBING]);
    assert!(written(&rig.advance(200)).is_empty());
}

#[test]
fn a_missed_probe_in_a_file_holds_the_job_at_the_probe_line() {
    let mut rig = buffered();
    probing_job(&mut rig);
    rig.rx("ok");
    rig.rx("ok");
    rig.rx("probing for z axis zero");
    let missed = rig.rx("error: probe did not connect");
    assert!(alerts(&missed).iter().any(|alert| alert.contains(MISSED)), "{:?}", alerts(&missed));
    ["program stopped", "z axis not set", ""].iter().for_each(|line| {
        rig.rx(line);
    });
    assert_eq!(flags(&rig), (0, true, 2, false));
    assert_eq!(rig.machine.snapshot().pause_button, wc_protocol::main::PauseButton::Resume);
    assert!(written(&rig.advance(300)).is_empty(), "the rest of the file waits");
    rig.clear_written();
    rig.act("startRun");
    assert_eq!(rig.run_to_quiet(), ["~"], "the missed probe left the bit's height unknown");
    rig.rx("<Idle,MPos:0.00,0.00,-10.00,WPos:0.000,0.000,0.000>");
    let restart = rig.run_to_quiet();
    assert!(restart.contains(&"G38.2 Z-10 F100".to_string()), "Play probes again: {restart:?}");
}

#[test]
fn a_probe_that_finishes_without_zeroing_alerts() {
    let mut rig = Rig::connected();
    rig.act("touchZ");
    rig.rx("G38.2 gcode only valid with z-axis attached");
    assert!(alerts(&rig.rx("ok")).iter().any(|alert| alert.contains(NOT_ZEROED)));
    assert!(!rig.machine.snapshot().probing);
    let mut touched = Rig::connected();
    touched.act("touchZ");
    touched.rx("z axis zeroed");
    assert!(alerts(&touched.rx("ok")).is_empty());
}

#[test]
fn a_probe_in_a_file_that_does_not_zero_pauses_the_job() {
    let mut rig = buffered();
    probing_job(&mut rig);
    rig.rx("ok");
    rig.rx("ok");
    rig.rx("probing for z axis zero");
    assert!(alerts(&rig.rx("ok")).iter().any(|alert| alert.contains(NOT_ZEROED)));
    assert_eq!(flags(&rig), (2, true, 3, false));
    assert!(written(&rig.advance(300)).is_empty());
    rig.clear_written();
    rig.act("resumeRun");
    rig.run_to_quiet();
    rig.rx("<Idle,MPos:0.00,0.00,-2.00,WPos:0.000,0.000,0.000>");
    let resumed = rig.run_to_quiet();
    assert!(resumed.iter().any(|line| line.starts_with("G1 Z5.0")), "Z stayed where it was: {resumed:?}");
}

#[test]
fn harmless_actions_run_during_a_probe() {
    let mut rig = Rig::connected();
    rig.act("touchZ");
    let has = |effects: &[Effect], wanted: &Effect| effects.contains(wanted);
    let shutdown = rig.act("shutdown");
    assert!(alerts(&shutdown).is_empty() && has(&shutdown, &Effect::Exit));
    assert!(has(&rig.act("TurnOffRPI"), &Effect::PowerOff));
    assert!(has(&rig.act("updatePorts"), &Effect::ListPorts));
    assert!(has(&rig.act("clearLogs"), &Effect::ClearLogs));
    assert!(has(&rig.act_with("createDirectory", json!("jobs"), json!("")), &Effect::CreateDirectory("jobs".into())));
    ["boardProcessGCode", "boardClearBoard"].iter().for_each(|command| {
        assert_ne!(alerts(&rig.act(command)), [PROBING], "{command}");
    });
    ["home", "moveZ", "defineZ0", "macro1", "setSprockets", "testMotors"].iter().for_each(|command| {
        assert_eq!(alerts(&rig.act(command)), [PROBING], "{command}");
    });
}

#[test]
fn the_status_message_says_when_a_probe_is_in_flight() {
    let mut rig = Rig::connected();
    let started = rig.act("touchZ");
    assert_eq!(pushed(&started, "statusMessage")[0]["probing"], true);
    rig.rx("z axis zeroed");
    let answered = rig.rx("ok");
    assert_eq!(pushed(&answered, "statusMessage")[0]["probing"], false);
    assert_eq!(rig.machine.snapshot().state_fields()["probing"], false);
}

/// A touching probe zeroes Z, so a Pause right after it must not replunge to the probe's depth.
#[test]
fn resume_after_a_probe_in_a_file_lowers_to_zero_not_the_probe_depth() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&["G21", "G0 Z5", "G38.2 Z-10 F100", "G0 X10", "G0 X20"]));
    rig.act("startRun");
    (0..2).for_each(|_| {
        rig.advance(100);
        rig.rx("ok");
    });
    rig.advance(100);
    rig.rx("z axis zeroed");
    rig.rx("ok");
    rig.act("pauseRun");
    rig.advance(100);
    rig.rx("ok");
    rig.advance(100);
    rig.clear_written();
    rig.act("resumeRun");
    let resumed = rig.run_to_quiet();
    assert!(resumed.iter().any(|line| line.starts_with("G1 Z0.0")), "{resumed:?}");
    assert!(!resumed.iter().any(|line| line.contains("Z-10")), "{resumed:?}");
}

#[test]
fn pause_during_a_probe_in_a_file_resumes_at_the_zeroed_z() {
    let mut rig = buffered();
    probing_job(&mut rig);
    rig.rx("ok");
    rig.rx("ok");
    rig.act("pauseRun");
    rig.rx("z axis zeroed");
    rig.rx("ok");
    rig.clear_written();
    rig.act("resumeRun");
    let resumed = rig.run_to_quiet();
    assert!(resumed.iter().any(|line| line.starts_with("G1 Z0.0")), "{resumed:?}");
}
