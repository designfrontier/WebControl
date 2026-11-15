//! The CPU work the machine hands the shell: one of each kind at a time, the newest request waiting behind it, and
//! each result pushed when it comes back.

mod rig;

use rig::{Rig, action, alerts, pushed, settings};
use serde_json::json;
use wc_machine::{Done, Effect, Event, Work};

fn report(rig: &mut Rig, left: f64) -> Vec<Effect> {
    let line = format!("[PE:{left},-1.0,120,1650.0,1651.0]");
    rig.machine.handle(Event::ControllerLine(line), rig.now)
}

fn work(effects: &[Effect]) -> Vec<Work> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Compute(work) => Some(work.clone()),
            _ => None,
        })
        .collect()
}

fn finish(rig: &mut Rig, work: Work) -> Vec<Effect> {
    rig.machine.handle(work.run(), rig.now)
}

fn computing() -> Rig {
    Rig::connected_with(settings(&[("WebControl Settings", "computedPosition", json!(1))]))
}

fn computed_y(effects: &[Effect]) -> f64 {
    pushed(effects, "errorValueMessage")[0]["computedY"].as_f64().unwrap()
}

#[test]
fn an_error_report_is_solved_off_the_task_and_pushed_when_the_solve_comes_back() {
    let mut rig = computing();
    let effects = report(&mut rig, 0.5);
    assert!(pushed(&effects, "errorValueMessage").is_empty());
    let solve = work(&effects);
    assert_eq!(solve.len(), 1);
    let answer = pushed(&finish(&mut rig, solve[0].clone()), "errorValueMessage");
    assert_eq!((&answer[0]["leftError"], &answer[0]["computedEnabled"]), (&json!(0.25), &json!(1)));
    assert_eq!(answer[0]["computedY"], json!(126.40874481201172), "the firmware's float32 solve");
}

#[test]
fn reports_during_a_solve_wait_and_only_the_newest_is_solved_next() {
    let mut rig = computing();
    let first = work(&report(&mut rig, 0.2));
    assert!(work(&report(&mut rig, 0.4)).is_empty(), "one solve at a time");
    let waiting = report(&mut rig, 0.6);
    assert!(work(&waiting).is_empty() && pushed(&waiting, "errorValueMessage").is_empty());
    let done = finish(&mut rig, first[0].clone());
    assert_eq!(pushed(&done, "errorValueMessage")[0]["leftError"], 0.1);
    let next = work(&done);
    assert!(matches!(&next[..], [Work::ComputedPosition { report, .. }] if report.left_error == 0.3), "{next:?}");
    let done = finish(&mut rig, next[0].clone());
    assert_eq!(pushed(&done, "errorValueMessage")[0]["leftError"], 0.3);
    assert!(work(&done).is_empty());
    assert_eq!(work(&report(&mut rig, 0.8)).len(), 1, "the lane is free again");
}

/// Every `$` write moves the model; a solve begun before one must not put the old model back.
#[test]
fn a_setting_written_during_a_solve_is_kept_and_the_waiting_solve_uses_it() {
    let mut rig = computing();
    let before = computed_y(&rig.send(Event::ControllerLine("[PE:0.9,-1.0,120,1650.0,1651.0]".into())));
    let running = work(&report(&mut rig, 0.2));
    report(&mut rig, 0.4);
    let body = json!({"values": {"motorOffsetY": "500"}});
    rig.send(Event::Edit { id: 1, request: wc_machine::Request::UpdateSettings { section: "maslow".into(), body } });
    assert_eq!(rig.run_to_quiet(), ["$3=500"]);
    let done = finish(&mut rig, running[0].clone());
    let waiting = work(&done);
    let lift = computed_y(&finish(&mut rig, waiting[0].clone())) - before;
    assert!((lift - 37.0).abs() < 1.0, "the waiting solve sees the new offset: {lift}");
    let lift = computed_y(&rig.send(Event::ControllerLine("[PE:0.1,-1.0,120,1650.0,1651.0]".into()))) - before;
    assert!((lift - 37.0).abs() < 1.0, "and so does the next: {lift}");
}

#[test]
fn a_goto_reads_the_file_off_the_task() {
    let mut rig = Rig::new();
    rig.machine.load_gcode("G0 X1\nG0 X2\nG0 X3\n");
    let effects = rig.machine.handle(action("moveGcodeGoto", json!(2), json!("")), rig.now);
    assert!(pushed(&effects, "gcodePositionMessage").is_empty());
    assert_eq!(rig.machine.snapshot().gcode_index, 2, "the index moves at once");
    let read = work(&effects);
    assert_eq!(
        read,
        [Work::GcodePosition {
            gcode: rig.machine.snapshot().gcode.0.clone(),
            index: 2,
            scale: 1.0,
            failure: Some("Error with moving to Z move".into()),
        }]
    );
    let position = pushed(&finish(&mut rig, read[0].clone()), "gcodePositionMessage");
    assert_eq!(position, [json!({"xval": 2.0, "yval": 0.0, "zval": 0.0, "gcodeLine": "G0 X3 ", "gcodeLineIndex": 2})]);
}

#[test]
fn a_failed_read_alerts_only_when_asked_to() {
    let mut rig = Rig::new();
    let failed = |failure: Option<&str>| Event::Computed(Done::GcodePosition(Err(failure.map(String::from))));
    assert_eq!(alerts(&rig.send(failed(Some("Error with moving to index")))), ["Error with moving to index"]);
    assert!(alerts(&rig.send(failed(None))).is_empty());
    let unreadable =
        Work::GcodePosition { gcode: vec!["G0 X`1".to_string()].into(), index: 1, scale: 1.0, failure: None };
    assert_eq!(unreadable.run(), failed(None));
}

#[test]
fn work_is_compared_by_value() {
    let line = || Event::ControllerLine("[PE:0.2,-1.0,120,1650.0,1651.0]".into());
    let (mut one, mut other) = (computing(), computing());
    let (first, second) = (one.machine.handle(line(), one.now), other.machine.handle(line(), other.now));
    assert_eq!(work(&first), work(&second));
    assert_ne!(work(&first)[0], Work::GcodePosition { gcode: Vec::new().into(), index: 0, scale: 1.0, failure: None });
}

#[test]
fn a_resync_reads_the_gcode_position_off_the_task_for_that_client_alone() {
    let mut rig = Rig::new();
    rig.machine.load_gcode("G0 X1\nG0 X2\nG0 X3\n");
    rig.machine.handle(action("moveGcodeGoto", json!(1), json!("")), rig.now);
    let goto = work(&rig.machine.handle(action("moveGcodeGoto", json!(2), json!("")), rig.now));
    assert!(goto.is_empty(), "the goto waits behind the one before it");
    let effects = rig.machine.handle(Event::ClientResync { sid: "s1".into() }, rig.now);
    let resync = work(&effects);
    assert_eq!(resync.len(), 1, "the resync does not wait behind the gotos");
    let done = rig.machine.handle(resync[0].clone().run(), rig.now);
    let to_s1: Vec<_> = done
        .iter()
        .filter_map(|e| match e {
            Effect::Push(p) if p.data["command"] == "gcodePositionMessage" => Some(p.client.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(to_s1, [Some("s1".to_string())]);
    assert!(work(&done).is_empty(), "and leaves the gotos' lane alone");
}
