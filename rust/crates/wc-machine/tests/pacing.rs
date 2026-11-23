//! Adjustable line pacing (`spec/firmware.md` B3): the two modes at each gap, a change mid-job, realtime writes, and
//! the per-job and per-segment statistics with their `PACING` log lines.

mod rig;

use std::collections::HashMap;

use rig::sim::SimRig;
use rig::{Rig, pushed, settings, written};
use serde_json::{Value, json};
use wc_machine::{Effect, Event, Request, Response};
use wc_testkit::contract::{Contract, ResponseBody};

const ARC: &str = "G3 X-126.4777 Y29.5677 I0.548 J21.8";
const STILL: &str = "<Idle,MPos:-140.22,25.13,-6.01,WPos:0.000,0.000,0.000>";
const EMPTY: &str = "[PE:-0.00,-0.01,127,1590.41,1822.64]";

fn paced(mode: &str, gap: u64) -> Rig {
    let mut rig = Rig::connected_with(settings(&[
        ("WebControl Settings", "linePacingMode", json!(mode)),
        ("WebControl Settings", "linePacingGapMs", json!(gap)),
    ]));
    rig.advance(500);
    rig
}

fn file(lines: &[&str]) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
}

fn respond(rig: &mut Rig, request: Request) -> (Response, Vec<Effect>) {
    let effects = rig.send(Event::Edit { id: 3, request });
    let response = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::Respond { id: 3, response } => Some(response.clone()),
            _ => None,
        })
        .expect("answered");
    (response, effects)
}

/// `PUT /api/run/pacing`, its answer checked against the contract.
fn set_pacing(rig: &mut Rig, mode: &str, gap: Value) -> (Response, Vec<Effect>) {
    let body = json!({"values": {"linePacingMode": mode, "linePacingGapMs": gap}});
    let (response, effects) = respond(rig, Request::Pacing(body));
    let checked = ResponseBody::Json(response.body.clone());
    Contract::shared().validate_response("PUT", "/api/run/pacing", response.status, &checked).unwrap();
    (response, effects)
}

/// Milliseconds from the first job line's write to the second's, the first acked 10 ms after it went out.
fn second_line_after(mode: &str, gap: u64) -> u64 {
    let mut rig = paced(mode, gap);
    rig.machine.load_gcode(&file(&["G0 X1", "G0 X2"]));
    assert_eq!(written(&rig.act("startRun")), ["G0 X1"]);
    rig.advance(10);
    if written(&rig.rx("ok")) == ["G0 X2"] {
        return 10;
    }
    (2..=30).find(|_| written(&rig.advance(10)) == ["G0 X2"]).expect("the second line went out") * 10
}

#[test]
fn v094_pacing_waits_a_full_gap_from_when_the_line_was_due() {
    let after: Vec<u64> = [0, 25, 50, 100].into_iter().map(|gap| second_line_after("v0.94", gap)).collect();
    assert_eq!(after, [10, 40, 60, 110], "due at 10 ms, then the gap in 10 ms ticks");
}

#[test]
fn minimum_gap_pacing_waits_only_until_the_gap_since_the_last_write() {
    let after: Vec<u64> = [0, 25, 50, 100].into_iter().map(|gap| second_line_after("minimum gap", gap)).collect();
    assert_eq!(after, [10, 30, 50, 100]);
}

#[test]
fn a_change_mid_job_applies_to_the_line_waiting_out_the_old_gap() {
    let mut rig = paced("v0.94", 100);
    rig.machine.load_gcode(&file(&["G0 X1", "G0 X2", "G0 X3"]));
    rig.act("startRun");
    rig.advance(10);
    assert!(written(&rig.rx("ok")).is_empty(), "waits until 110 ms");
    let (response, effects) = set_pacing(&mut rig, "minimum gap", json!(0));
    assert_eq!(response.status, 200);
    assert_eq!(written(&effects), ["G0 X2"], "the next line goes at once");
    let (_, effects) = set_pacing(&mut rig, "minimum gap", json!(100));
    assert!(written(&effects).is_empty());
    rig.advance(10);
    assert!(written(&rig.rx("ok")).is_empty(), "now 100 ms after the last write");
    assert!(written(&rig.advance(80)).is_empty());
    assert_eq!(written(&rig.advance(10)), ["G0 X3"]);
}

#[test]
fn realtime_writes_are_never_paced_in_either_mode() {
    for mode in ["v0.94", "minimum gap"] {
        let mut rig = paced(mode, 500);
        rig.act_with("move", json!("up"), json!("1"));
        rig.advance(10);
        rig.rx("ok");
        rig.rx("Maslow Paused");
        assert_eq!(written(&rig.act("resumeRun")), ["~"], "{mode}");
        assert_eq!(written(&rig.act("stopRun")), ["!"], "{mode}");
    }
}

#[test]
fn the_pacing_settings_are_checked_like_any_other_setting() {
    let mut rig = Rig::connected();
    let put = |rig: &mut Rig, values: Value| {
        let request = Request::UpdateSettings { section: "webcontrol".into(), body: json!({ "values": values }) };
        respond(rig, request).0
    };
    let refused = [
        (json!({"linePacingGapMs": "501"}), "Must be between 0 and 500"),
        (json!({"linePacingGapMs": "-1"}), "Must be between 0 and 500"),
        (json!({"linePacingGapMs": "25.5"}), "Must be a whole number"),
        (json!({"linePacingMode": "fastest"}), "Must be one of the listed options"),
    ];
    refused.into_iter().for_each(|(values, error)| {
        let key = values.as_object().unwrap().keys().next().unwrap().clone();
        assert_eq!(put(&mut rig, values).body["errors"][&key], error);
    });
    assert_eq!(put(&mut rig, json!({"linePacingGapMs": "500", "linePacingMode": "minimum gap"})).status, 200);
    let stored = |rig: &Rig, key: &str| rig.machine.snapshot().settings.value("WebControl Settings", key).cloned();
    assert_eq!(stored(&rig, "linePacingGapMs"), Some(json!(500)));
    let (response, _) = set_pacing(&mut rig, "v0.94", json!(501));
    assert_eq!(
        (response.status, response.body["errors"]["linePacingGapMs"].clone()),
        (400, json!("Must be between 0 and 500"))
    );
    let other = json!({"values": {"linePacingGapMs": 5, "bufferOn": 1}});
    assert_eq!(respond(&mut rig, Request::Pacing(other)).0.body["errors"]["bufferOn"], "Not a line pacing setting");
    assert_eq!(respond(&mut rig, Request::Pacing(json!([]))).0.status, 400);
    assert_eq!(stored(&rig, "linePacingGapMs"), Some(json!(500)), "nothing refused was stored");
}

#[test]
fn the_pacing_endpoint_works_during_a_job_where_the_settings_screens_are_locked() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&["G0 X1", "G0 X2"]));
    rig.act("startRun");
    let screen =
        Request::UpdateSettings { section: "webcontrol".into(), body: json!({"values": {"linePacingGapMs": "0"}}) };
    assert_eq!(respond(&mut rig, screen).0.status, 409);
    let (response, _) = set_pacing(&mut rig, "minimum gap", json!(25));
    assert_eq!((response.body["mode"].clone(), response.body["gapMs"].clone()), (json!("minimum gap"), json!(25)));
}

/// `PACING <LABEL WORDS> key=value ...` as a log reader would split it: every word after the label is a pair.
fn parse(line: &str) -> (String, HashMap<String, String>) {
    let words: Vec<&str> = line.split(' ').collect();
    assert_eq!(words[0], "PACING");
    let label = words[1..].iter().take_while(|word| !word.contains('=')).copied().collect::<Vec<_>>().join(" ");
    let pairs = &words[1 + label.split(' ').count()..];
    let fields = pairs.iter().map(|word| word.split_once('=').expect(line)).map(|(k, v)| (k.into(), v.into()));
    (label, fields.collect())
}

fn logged(effects: &[Effect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Log(line) if line.starts_with("PACING") => Some(line.clone()),
            _ => None,
        })
        .collect()
}

fn console(effects: &[Effect]) -> Vec<String> {
    pushed(effects, "controllerMessage").iter().filter_map(|text| text.as_str().map(String::from)).collect()
}

/// Acks each job line 10 ms after it goes out, for `ms`.
fn stream(rig: &mut Rig, ms: u64) -> Vec<Effect> {
    (0..ms / 10)
        .flat_map(|_| {
            let mut effects = rig.advance(10);
            if written(&effects).iter().any(|line| line.starts_with("G0")) {
                effects.extend(rig.advance(10));
                effects.extend(rig.rx("ok"));
            }
            effects
        })
        .collect()
}

#[test]
fn a_pacing_change_ends_the_segment_with_a_parsable_summary_and_marks_the_new_one() {
    let mut rig = Rig::connected();
    let lines: Vec<String> = (1..=8).map(|n| format!("G0 X{n}")).collect();
    rig.machine.load_gcode(&lines.join("\n"));
    let mut effects = rig.act("startRun");
    effects.extend(rig.advance(10));
    effects.extend(rig.rx("ok"));
    effects.extend(stream(&mut rig, 120));
    let (_, change) = set_pacing(&mut rig, "minimum gap", json!(25));
    effects.extend(change.clone());
    assert_eq!(written(&change), ["G0 X4"], "25 ms after the last write has passed");
    effects.extend(rig.advance(10));
    effects.extend(rig.rx("ok"));
    effects.extend(stream(&mut rig, 1000));
    effects.extend(rig.advance(100));
    let log = logged(&effects);
    let parsed: Vec<_> = log.iter().map(|line| parse(line)).collect();
    let labels: Vec<&str> = parsed.iter().map(|(label, _)| label.as_str()).collect();
    assert_eq!(labels, ["JOB START", "SEGMENT", "CHANGE", "SEGMENT", "JOB", "JOB END"]);
    assert_eq!(log[0], "PACING JOB START file_line=1 mode=v0.94 gap_ms=50");
    let first = &parsed[1].1;
    let field = |fields: &HashMap<String, String>, key: &str| fields[key].clone();
    assert_eq!(
        ["mode", "gap_ms", "lines", "lost", "lost_twice", "file_line_start", "file_line_end", "ended_by"]
            .map(|key| field(first, key)),
        ["v0.94", "50", "3", "0", "0", "1", "3", "pacing_change"]
    );
    assert_eq!(field(first, "median_short_gap_ms"), "60", "a 10 ms ack, then a full 50 ms");
    assert_eq!(log[2], "PACING CHANGE file_line=4 mode=minimum_gap gap_ms=25 from_mode=v0.94 from_gap_ms=50");
    let second = &parsed[3].1;
    assert_eq!(
        ["mode", "gap_ms", "lines", "file_line_start", "file_line_end", "ended_by"].map(|key| field(second, key)),
        ["minimum_gap", "25", "5", "4", "8", "job_end"]
    );
    assert_eq!(field(second, "median_short_gap_ms"), "30", "a 10 ms ack, then 25 ms since the write in 10 ms ticks");
    let job = &parsed[4].1;
    assert_eq!((field(job, "lines"), field(job, "mode")), ("8".into(), "v0.94".into()));
    let times = "wall_s=0.3 cutting_s=0.3 estimated_s=0.7 file=none";
    assert_eq!(log[5], format!("PACING JOB END file_line_end=8 mode=minimum_gap gap_ms=25 ended_by=job_end {times}"));
    assert_eq!(
        console(&effects).iter().filter(|text| text.starts_with("PACING")).collect::<Vec<_>>(),
        log.iter().collect::<Vec<_>>()
    );
    let stats = &pushed(&change, "pacingStats")[0];
    assert_eq!((stats["history"][0]["lines"].clone(), stats["segment"]["lines"].clone()), (json!(3), json!(1)));
    let state = (*rig.machine.snapshot().pacing.0).clone();
    assert_eq!((state.job.unwrap().lines, state.segment, state.history.len()), (8, None, 2));
}

/// The firmware's reports every 200 ms for `ms`.
fn reports(rig: &mut Rig, ms: u64) -> Vec<Effect> {
    (0..ms / 200)
        .flat_map(|_| {
            let mut effects = rig.advance(200);
            effects.extend(rig.rx(STILL));
            effects.extend(rig.rx(EMPTY));
            effects
        })
        .collect()
}

#[test]
fn a_lost_line_resent_and_a_line_lost_twice_are_counted_with_their_recovery_time() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&["G1 X1", ARC, "G1 X3", "G1 X4"]));
    rig.act("startRun");
    rig.rx("G1 X1");
    rig.rx("ok");
    assert_eq!(written(&rig.advance(100)), [ARC]);
    assert_eq!(written(&reports(&mut rig, 600)), [ARC], "resent on the third empty report");
    rig.rx(ARC);
    rig.rx("ok");
    assert_eq!(written(&rig.advance(100)), ["G1 X3"]);
    let effects = reports(&mut rig, 600);
    assert_eq!(written(&effects), ["G1 X3"]);
    let effects = reports(&mut rig, 600);
    assert_eq!(rig.machine.snapshot().upload_flag, 2, "lost twice, the job pauses");
    assert!(written(&effects).is_empty());
    let effects = [rig.advance(2000), rig.act("stopRun")].concat();
    let summary = logged(&effects).into_iter().find(|line| line.starts_with("PACING SEGMENT")).unwrap();
    let (_, fields) = parse(&summary);
    let numbers = ["lines", "lost", "lost_twice"].map(|key| fields[key].clone());
    assert_eq!(numbers, ["3", "3", "1"], "the ARC once, G1 X3 twice");
    assert_eq!((fields["ended_by"].as_str(), fields["lost_pct"].as_str()), ("stop", "100"));
    let recovery: f64 = fields["recovery_s"].parse().unwrap();
    assert!((1.0..1.5).contains(&recovery), "two waits of about 0.6 s: {recovery}");
    let duration: f64 = fields["duration_s"].parse().unwrap();
    assert!(duration < 2.5, "the pause is not streaming time: {duration}");
    let job = rig.machine.snapshot().pacing.0.job.clone().unwrap();
    assert_eq!((job.lost, job.lost_twice, job.ended_by.as_deref()), (3, 1, Some("Stop")));
}

#[test]
fn the_stats_are_pushed_every_2_s_during_a_job_and_resent_on_resync() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&["G1 X1", "G1 X2"]));
    let started = rig.act("startRun");
    assert_eq!(pushed(&started, "pacingStats")[0]["segment"]["lines"], 1);
    let pushes = pushed(&rig.advance(4000), "pacingStats");
    assert_eq!(pushes.len(), 2);
    assert!(pushes[1]["segment"]["elapsedS"].as_f64().unwrap() >= 4.0);
    assert!(pushed(&rig.send(Event::ClientResync { sid: "s1".into() }), "pacingStats").len() == 1);
    rig.act("stopRun");
    assert!(pushed(&rig.advance(4000), "pacingStats").is_empty(), "nothing outside a job");
}

#[test]
fn the_job_timer_leaves_pauses_out_of_cutting_time_and_scales_the_estimate_by_the_real_pace() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&["G1 X10 F60", "G1 X20", "G1 X30", "G1 X40"]));
    let job = |effects: &[Effect]| pushed(effects, "pacingStats").last().expect("pushed")["job"].clone();
    let started = rig.act("startRun");
    assert_eq!(pushed(&started, "pacingStats")[0]["estimateS"], 40.0);
    assert_eq!(job(&started)["remainingS"], 40.0, "nothing done yet, so the estimate as it is");
    rig.advance(1000);
    rig.rx("ok");
    let paced = job(&rig.advance(2000));
    assert_eq!(paced["remainingS"], 6.0, "10 s of estimate done in 2 s, so the 30 s left reads as 6 s");
    rig.act("pauseRun");
    rig.rx("ok");
    reports(&mut rig, 10_000);
    let mut effects = rig.act("resumeRun");
    effects.extend((0..20).flat_map(|_| [rig.advance(500), rig.rx("ok")].concat()));
    let end = logged(&effects).into_iter().find(|line| line.starts_with("PACING JOB END")).expect("the job ended");
    let (_, fields) = parse(&end);
    let seconds = |key: &str| fields[key].parse::<f64>().unwrap();
    assert!(seconds("wall_s") - seconds("cutting_s") >= 10.0, "the pause is wall time only: {end}");
    assert_eq!((fields["estimated_s"].as_str(), fields["file"].as_str()), ("40.1", "none"));
    let ended = job(&effects);
    assert_eq!(ended["remainingS"], Value::Null);
    assert!(ended["wallS"].as_f64().unwrap() - ended["elapsedS"].as_f64().unwrap() >= 10.0);
}

fn buffered_sim(mode: &str, gap: u64) -> SimRig {
    let mut rig = SimRig::with(settings(&[
        ("Maslow Settings", "zAxis", json!(1)),
        ("Maslow Settings", "bufferOn", json!(1)),
        ("WebControl Settings", "linePacingMode", json!(mode)),
        ("WebControl Settings", "linePacingGapMs", json!(gap)),
    ]));
    rig.sim.move_ms = 200;
    rig
}

fn job_lines(count: usize) -> Vec<String> {
    let mut lines: Vec<String> = (0..count).map(|n| format!("G1 X{n} Y{} F800", n % 7)).collect();
    lines.insert(count / 2, ARC.to_string());
    lines
}

/// What the simulator received of the job, in order, realtime and setup lines aside.
fn job_written(rig: &SimRig, lines: &[String]) -> Vec<String> {
    rig.written.iter().map(|line| line.trim_end().to_string()).filter(|line| lines.contains(line)).collect()
}

#[test]
fn a_zero_gap_still_fills_the_ring_only_as_far_as_the_byte_accounting_allows() {
    let mut rig = buffered_sim("minimum gap", 0);
    let lines = job_lines(60);
    rig.machine.load_gcode(&lines.join("\n"));
    rig.take();
    rig.act("startRun");
    rig.run(30_000);
    assert!(!rig.machine.snapshot().job_in_progress);
    assert_eq!(rig.sim.overflows(), 0);
    assert!((61..=127).contains(&rig.sim.max_unread()), "full but never over: {}", rig.sim.max_unread());
    assert_eq!(job_written(&rig, &lines), lines, "each line once, in order");
}

#[test]
fn pacing_changes_every_few_hundred_ms_mid_job_neither_skip_nor_repeat_a_line() {
    let mut rig = buffered_sim("v0.94", 50);
    let lines = job_lines(80);
    rig.machine.load_gcode(&lines.join("\n"));
    rig.take();
    rig.act("startRun");
    let pacings = [("minimum gap", 0), ("v0.94", 100), ("minimum gap", 25), ("v0.94", 0), ("minimum gap", 500)];
    (0..40).for_each(|n| {
        let (mode, gap) = pacings[n % pacings.len()];
        let body = json!({"values": {"linePacingMode": mode, "linePacingGapMs": gap}});
        rig.send(Event::Edit { id: 9, request: Request::Pacing(body) });
        rig.run(300);
    });
    rig.run(60_000);
    assert!(!rig.machine.snapshot().job_in_progress);
    assert_eq!(rig.sim.overflows(), 0);
    assert_eq!(job_written(&rig, &lines), lines);
    assert!(!rig.written.iter().any(|line| line.starts_with('$')), "no setting reached the controller mid-job");
    let segments = (*rig.machine.snapshot().pacing.0).clone();
    assert_eq!(segments.job.unwrap().lines, lines.len() as u64);
}

#[test]
fn stop_goes_out_at_once_at_every_gap_in_both_modes() {
    [("v0.94", 0), ("v0.94", 500), ("minimum gap", 0), ("minimum gap", 500)].into_iter().for_each(|(mode, gap)| {
        let mut rig = paced(mode, gap);
        rig.machine.load_gcode(&file(&["G0 X1", "G0 X2", "G0 X3"]));
        rig.act("startRun");
        assert_eq!(written(&rig.act("stopRun")), ["!"], "{mode} {gap}");
    });
}

#[test]
fn a_pacing_change_while_a_line_waits_for_its_ack_leaves_the_lost_line_watch_alone() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&["G1 X1", ARC, "G1 X3"]));
    rig.act("startRun");
    rig.rx("G1 X1");
    rig.rx("ok");
    assert_eq!(written(&rig.advance(100)), [ARC]);
    let (_, effects) = set_pacing(&mut rig, "minimum gap", json!(0));
    assert!(written(&effects).is_empty(), "the arc still waits for its ack");
    assert_eq!(written(&reports(&mut rig, 400)), Vec::<String>::new(), "two empty reports are not enough");
    assert_eq!(written(&reports(&mut rig, 200)), [ARC], "the third sends it again, as with no change");
    rig.rx(ARC);
    assert_eq!(written(&rig.rx("ok")), ["G1 X3"], "then the next line, at once with no gap");
}

#[test]
fn the_wait_before_a_job_is_not_a_short_line_gap() {
    let mut rig = Rig::connected();
    rig.advance(3000);
    rig.machine.load_gcode(&file(&["G0 X1"]));
    rig.act("startRun");
    rig.advance(10);
    rig.rx("ok");
    rig.advance(100);
    let job = rig.machine.snapshot().pacing.0.job.clone().unwrap();
    assert_eq!((job.lines, job.median_short_gap_ms), (1, None), "the line followed no write of the job's");
}
