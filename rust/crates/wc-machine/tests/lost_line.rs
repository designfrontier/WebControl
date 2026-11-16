//! A line the controller never received (`spec/firmware.md` B18): no echo, no `ok`, an empty ring and a still sled
//! for 2 s. It is sent again once; lost again, the job pauses at it. Nothing that runs a line is taken for one.

mod rig;

use rig::{Rig, alerts, pushed, settings, written};
use serde_json::json;
use wc_machine::Effect;

const ARC: &str = "G3 X-126.4777 Y29.5677 I0.548 J21.8";
const STILL: &str = "<Idle,MPos:-140.22,25.13,-6.01,WPos:0.000,0.000,0.000>";
const EMPTY: &str = "[PE:-0.00,-0.01,127,1590.41,1822.64]";
const NOT_RECEIVED: &str = "The controller did not receive line 2";

fn file(lines: &[&str]) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
}

fn buffered() -> Rig {
    Rig::connected_with(settings(&[("Maslow Settings", "bufferOn", json!(1))]))
}

/// The firmware's status and `[PE:]` reports every 200 ms for `ms`; returns every effect meanwhile.
fn reports(rig: &mut Rig, ms: u64, status: &str, pe: &str) -> Vec<Effect> {
    (0..ms / 200)
        .flat_map(|_| {
            let mut effects = rig.advance(200);
            effects.extend(rig.rx(status));
            effects.extend(rig.rx(pe));
            effects
        })
        .collect()
}

/// The status reports of a sled that moves 1 mm in X between reports.
fn moving(rig: &mut Rig, ms: u64) -> Vec<Effect> {
    (0..ms / 200)
        .flat_map(|step| {
            let mut effects = rig.advance(200);
            effects.extend(rig.rx(&format!("<Idle,MPos:{step}.00,0.00,0.00,WPos:0.000,0.000,0.000>")));
            effects.extend(rig.rx(EMPTY));
            effects
        })
        .collect()
}

fn controller_messages(effects: &[Effect]) -> Vec<String> {
    pushed(effects, "controllerMessage").iter().map(|text| text.as_str().unwrap_or_default().to_string()).collect()
}

/// The `ok` and the 50 ms gap after it; returns what was written.
fn ok(rig: &mut Rig) -> Vec<String> {
    let mut effects = rig.rx("ok");
    effects.extend(rig.advance(100));
    written(&effects)
}

/// A 1 mm jog to the right; returns what it wrote.
fn jog(rig: &mut Rig) -> Vec<String> {
    let mut effects = rig.act_with("move", json!("right"), json!("1"));
    effects.extend(rig.advance(100));
    written(&effects)
}

/// Acks every line written, as a controller that runs them, until `target` goes out (unacked).
fn ack_until(rig: &mut Rig, mut pending: Vec<String>, target: &str) {
    for _ in 0..20 {
        if pending.iter().any(|line| line == target) {
            return;
        }
        let mut effects: Vec<Effect> = pending.iter().flat_map(|_| rig.rx("ok")).collect();
        effects.extend(rig.advance(100));
        pending = written(&effects);
    }
    panic!("{target} never went out");
}

/// Plays `lines`, echoing and acking the first; the second goes out and is never heard of.
fn lose_second_line(rig: &mut Rig, lines: &[&str]) {
    rig.machine.load_gcode(&file(lines));
    assert_eq!(written(&rig.act("startRun")), [lines[0]]);
    rig.rx(lines[0]);
    assert_eq!(ok(rig), [lines[1]]);
}

/// The line written last never arrives; 2 s of empty, still reports later it goes out again, once.
fn resent_after_two_seconds(rig: &mut Rig, line: &str) -> Vec<Effect> {
    assert!(written(&reports(rig, 1800, STILL, EMPTY)).is_empty(), "not before 2 s");
    let effects = reports(rig, 600, STILL, EMPTY);
    assert_eq!(written(&effects), [line]);
    effects
}

#[test]
fn a_line_the_controller_never_echoed_is_sent_again_once_and_the_job_goes_on() {
    let mut rig = Rig::connected();
    lose_second_line(&mut rig, &["G1 X1", ARC, "G1 X3"]);
    let effects = resent_after_two_seconds(&mut rig, ARC);
    let notes = controller_messages(&effects);
    assert!(notes.iter().any(|note| note.contains("Line 2") && note.contains(ARC)), "{notes:?}");
    assert!(alerts(&effects).is_empty());
    rig.rx(ARC);
    assert_eq!(ok(&mut rig), ["G1 X3"]);
    assert_eq!(rig.machine.snapshot().upload_flag, 1);
}

#[test]
fn a_status_report_that_moves_or_a_ring_that_holds_bytes_starts_the_wait_over() {
    let mut rig = Rig::connected();
    lose_second_line(&mut rig, &["G1 X1", ARC, "G1 X3"]);
    reports(&mut rig, 1800, STILL, EMPTY);
    rig.rx("<Idle,MPos:-140.21,25.13,-6.01,WPos:0.000,0.000,0.000>");
    rig.rx(EMPTY);
    reports(&mut rig, 1600, "<Idle,MPos:-140.21,25.13,-6.01,WPos:0.000,0.000,0.000>", EMPTY);
    rig.rx("[PE:-0.00,-0.01,89,1590.41,1822.64]");
    assert!(written(&reports(&mut rig, 1800, STILL, EMPTY)).is_empty());
    assert_eq!(written(&reports(&mut rig, 400, STILL, EMPTY)), [ARC]);
}

#[test]
fn a_long_move_is_never_taken_for_a_lost_line() {
    let mut rig = Rig::connected();
    lose_second_line(&mut rig, &["G1 X1", "G1 X500", ARC]);
    rig.rx("G1 X500");
    assert!(written(&reports(&mut rig, 4000, STILL, EMPTY)).is_empty(), "echoed: it runs, however still");
    let mut moved = Rig::connected();
    lose_second_line(&mut moved, &["G1 X1", "G1 X500", ARC]);
    assert!(written(&moving(&mut moved, 4000)).is_empty(), "the sled moves: it runs, echo or not");
    assert_eq!(ok(&mut rig), [ARC]);
    resent_after_two_seconds(&mut rig, ARC);
}

#[test]
fn a_dwell_is_never_taken_for_a_lost_line() {
    let mut rig = Rig::connected();
    lose_second_line(&mut rig, &["G1 X1", "G4 P6000", ARC]);
    rig.rx("G4 P6000");
    assert!(written(&reports(&mut rig, 4600, STILL, EMPTY)).is_empty());
    assert_eq!(ok(&mut rig), [ARC]);
    resent_after_two_seconds(&mut rig, ARC);
}

#[test]
fn an_m0_pause_is_never_taken_for_a_lost_line() {
    let mut rig = Rig::connected();
    lose_second_line(&mut rig, &["G1 X1", "M0", ARC]);
    rig.rx("M0");
    rig.rx("Maslow Paused");
    let paused = "<Pause,MPos:-140.22,25.13,-6.01,WPos:0.000,0.000,0.000>";
    assert!(written(&reports(&mut rig, 4000, paused, EMPTY)).is_empty());
    assert_eq!(written(&rig.act("resumeRun")), ["~"]);
    let mut effects = rig.rx("ok");
    effects.extend(rig.rx("ok"));
    effects.extend(rig.advance(100));
    effects.extend(rig.rx(STILL));
    ack_until(&mut rig, written(&effects), ARC);
    resent_after_two_seconds(&mut rig, ARC);
}

#[test]
fn an_m0_whose_echo_was_lost_is_never_taken_for_a_lost_line() {
    let mut rig = Rig::connected();
    lose_second_line(&mut rig, &["G1 X1", "M0", ARC]);
    rig.rx("Maslow Paused");
    assert!(written(&reports(&mut rig, 4000, STILL, EMPTY)).is_empty());
    let mut lost = Rig::connected();
    lose_second_line(&mut lost, &["G1 X1", "M0", ARC]);
    resent_after_two_seconds(&mut lost, "M0");
}

#[test]
fn a_probe_is_never_taken_for_a_lost_line() {
    let mut rig = Rig::connected();
    rig.act("touchZ");
    rig.rx("G90");
    rig.rx("G38.2 Z-25.4");
    rig.rx("probing for z axis zero");
    assert!(written(&reports(&mut rig, 4000, STILL, EMPTY)).is_empty());
    rig.rx("z axis zeroed");
    rig.rx("ok");
    assert!(!rig.machine.snapshot().probing);
    let mut lost = Rig::connected();
    let probe = written(&lost.act("touchZ"));
    assert!(probe[0].contains("G38.2"), "{probe:?}");
    resent_after_two_seconds(&mut lost, &probe[0]);
    lost.rx("G90");
    lost.rx("z axis zeroed");
    let touched = lost.rx("ok");
    assert!(alerts(&touched).is_empty(), "the probe answered is the one sent again");
    assert!(!lost.machine.snapshot().probing);
}

#[test]
fn buffered_lines_behind_a_running_line_are_resent_only_once_it_finishes() {
    let mut rig = buffered();
    rig.machine.load_gcode(&file(&["G1 X500", "G1 X2", "G1 X3", "G1 X4"]));
    assert_eq!(written(&rig.act("startRun")), ["G1 X500"]);
    assert_eq!(written(&rig.advance(200)), ["G1 X2", "G1 X3", "G1 X4"]);
    rig.rx("G1 X500");
    assert!(written(&reports(&mut rig, 4000, STILL, EMPTY)).is_empty(), "the first line runs");
    assert!(ok(&mut rig).is_empty());
    rig.rx("G1 X2");
    assert!(written(&reports(&mut rig, 4000, STILL, "[PE:-0.00,-0.01,113,1590.41,1822.64]")).is_empty());
    rig.rx("ok");
    let effects = reports(&mut rig, 2400, STILL, EMPTY);
    assert_eq!(written(&effects), ["G1 X3", "G1 X4"], "only the lines never echoed, in order");
    assert_eq!(controller_messages(&effects).len(), 2);
    rig.rx("G1 X3");
    rig.rx("ok");
    rig.rx("G1 X4");
    rig.rx("ok");
    assert_eq!(rig.machine.snapshot().upload_flag, 0, "every byte answered: the job is done");
    assert_eq!(jog(&mut rig), ["G91"], "the accounting is whole again");
}

#[test]
fn a_line_lost_twice_pauses_the_job_at_it_and_resume_sends_it_again() {
    let mut rig = Rig::connected();
    lose_second_line(&mut rig, &["G21", ARC, "G1 X3"]);
    resent_after_two_seconds(&mut rig, ARC);
    assert!(written(&reports(&mut rig, 1800, STILL, EMPTY)).is_empty());
    let effects = reports(&mut rig, 600, STILL, EMPTY);
    assert!(written(&effects).is_empty(), "never a third time on its own");
    let alert = alerts(&effects);
    assert!(alert.len() == 1 && alert[0].contains(NOT_RECEIVED) && alert[0].contains(ARC), "{alert:?}");
    assert!(alert[0].contains("Press Resume to send it again, or Stop."));
    let snapshot = rig.machine.snapshot();
    assert_eq!((snapshot.upload_flag, snapshot.gcode_index), (2, 1));
    assert!(written(&reports(&mut rig, 3000, STILL, EMPTY)).is_empty());
    rig.clear_written();
    assert_eq!(written(&rig.act("resumeRun")), ["~"]);
    rig.rx("ok");
    rig.rx(STILL);
    let sent = rig.run_to_quiet();
    assert!(sent.contains(&ARC.to_string()), "{sent:?}");
    assert_eq!(rig.machine.snapshot().upload_flag, 0, "the job ran to its end");
}

#[test]
fn resume_while_waiting_for_a_lost_line_returns_to_it() {
    let mut rig = Rig::connected();
    lose_second_line(&mut rig, &["G21", ARC, "G1 X3"]);
    rig.act("pauseRun");
    let effects = reports(&mut rig, 2400, STILL, EMPTY);
    assert!(written(&effects).is_empty(), "paused: nothing moves until Resume");
    assert!(controller_messages(&effects).iter().any(|note| note.contains("Line 2") && note.contains("Resume")));
    assert_eq!(rig.machine.snapshot().gcode_index, 1);
    rig.clear_written();
    assert_eq!(written(&rig.act("resumeRun")), ["~"]);
    rig.rx("ok");
    rig.rx(STILL);
    let sent = rig.run_to_quiet();
    assert!(sent.contains(&ARC.to_string()) && sent.ends_with(&["G1 X3".to_string()]), "{sent:?}");
}

#[test]
fn resume_pressed_before_the_loss_is_seen_still_gets_the_line_sent() {
    let mut rig = Rig::connected();
    lose_second_line(&mut rig, &["G21", ARC, "G1 X3"]);
    rig.act("pauseRun");
    reports(&mut rig, 1000, STILL, EMPTY);
    assert_eq!(written(&rig.act("resumeRun")), ["~"]);
    rig.rx("ok");
    let effects = reports(&mut rig, 2400, STILL, EMPTY);
    assert_eq!(written(&effects), [ARC], "the restart waits behind it");
}

#[test]
fn stop_clears_a_lost_line_and_nothing_is_sent_again() {
    let mut rig = Rig::connected();
    lose_second_line(&mut rig, &["G1 X1", ARC, "G1 X3"]);
    reports(&mut rig, 1000, STILL, EMPTY);
    assert_eq!(written(&rig.act("stopRun")), ["!"]);
    rig.rx("ok");
    rig.rx("[Forward Calculating Position]");
    let effects = reports(&mut rig, 4000, STILL, EMPTY);
    assert!(written(&effects).is_empty() && alerts(&effects).is_empty());
    assert!(!rig.machine.snapshot().job_in_progress);
    assert_eq!(jog(&mut rig), ["G91"]);
    resent_after_two_seconds(&mut rig, "G91");
}

#[test]
fn a_tilde_waiting_for_its_ok_holds_the_check() {
    let mut rig = Rig::connected();
    lose_second_line(&mut rig, &["G1 X1", ARC, "G1 X3"]);
    rig.act("pauseRun");
    assert_eq!(written(&rig.act("resumeRun")), ["~"]);
    assert!(written(&reports(&mut rig, 4000, STILL, EMPTY)).is_empty());
    rig.rx("ok");
    assert_eq!(written(&reports(&mut rig, 2400, STILL, EMPTY)), [ARC]);
}

#[test]
fn a_lost_command_is_sent_again_and_one_lost_twice_is_dropped_with_what_follows() {
    let mut rig = Rig::connected();
    assert_eq!(jog(&mut rig), ["G91"]);
    let effects = resent_after_two_seconds(&mut rig, "G91");
    assert!(controller_messages(&effects).iter().any(|note| note.contains("did not receive G91")));
    rig.rx("G91");
    assert_eq!(ok(&mut rig), ["G00 X1.0"]);
    reports(&mut rig, 2400, STILL, EMPTY);
    let effects = reports(&mut rig, 2400, STILL, EMPTY);
    assert!(alerts(&effects).iter().any(|alert| alert.contains("did not receive G00 X1.0")), "{:?}", alerts(&effects));
    assert_eq!(written(&effects), ["G21"], "the firmware is set back to G21 and G90 instead of the jog's tail");
    rig.rx("G21");
    assert_eq!(ok(&mut rig), ["G90"]);
}

#[test]
fn a_restart_line_lost_twice_pauses_the_job_again() {
    let mut rig = Rig::connected();
    lose_second_line(&mut rig, &["G21", ARC, "G1 X3"]);
    rig.act("pauseRun");
    reports(&mut rig, 2400, STILL, EMPTY);
    assert_eq!(written(&rig.act("resumeRun")), ["~"]);
    let mut effects = rig.rx("ok");
    effects.extend(rig.advance(100));
    effects.extend(rig.rx(STILL));
    let first = written(&effects);
    assert_eq!(first.len(), 1, "{first:?}");
    resent_after_two_seconds(&mut rig, &first[0]);
    let effects = reports(&mut rig, 2400, STILL, EMPTY);
    assert!(alerts(&effects).iter().any(|alert| alert.contains("The job is paused")), "{:?}", alerts(&effects));
    assert_eq!(written(&effects), ["G21"]);
    let snapshot = rig.machine.snapshot();
    assert_eq!((snapshot.upload_flag, snapshot.gcode_index), (2, 1));
}

#[test]
fn a_tool_change_lost_twice_still_changes_the_tool_on_resume() {
    let mut rig = Rig::connected();
    lose_second_line(&mut rig, &["G21", "T2 M6", "G1 X3"]);
    resent_after_two_seconds(&mut rig, "T2 M6");
    assert!(!alerts(&reports(&mut rig, 2400, STILL, EMPTY)).is_empty());
    assert_eq!(rig.machine.snapshot().current_tool, 0, "the firmware never saw the M6");
    assert_eq!(written(&rig.act("resumeRun")), ["~"]);
    let mut effects = rig.rx("ok");
    effects.extend(rig.advance(100));
    effects.extend(rig.rx(STILL));
    ack_until(&mut rig, written(&effects), "T2 M6");
    rig.rx("T2");
    rig.rx("Tool change to tool 2");
    rig.rx("M6");
    rig.rx("Tool Change: Please insert tool 2");
    rig.rx("Maslow Paused");
    assert_eq!(rig.machine.snapshot().current_tool, 2);
}
