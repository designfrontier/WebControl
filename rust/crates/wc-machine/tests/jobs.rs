//! Jobs, ported from test_job_in_progress, test_controller_pause, test_program_pause, test_motion's resume tests and
//! test_tool_change_restart: Play, Pause, Resume, Stop, the controller's pauses and the restart sequence.

mod rig;

use rig::{Rig, alerts, pushed, settings, written};
use serde_json::{Value, json};
use wc_machine::files::{GCODE_LOCKED, Gcode, Origin, Request};
use wc_machine::{Effect, Event};

const PAUSED: &str =
    "Cannot issue command while paused sending gcode. You must press STOP before performing this action.";
const SAFE_Z: &str = "G0 Z5.0";

fn file(lines: &[&str]) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
}

/// Loads `lines` and plays until the index reaches `index`, acking each line, then pauses there.
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

fn flags(rig: &Rig) -> (i32, bool, usize) {
    let snapshot = rig.machine.snapshot();
    (snapshot.upload_flag, snapshot.job_in_progress, snapshot.gcode_index)
}

const JOB: [&str; 6] = ["G21", "G90", "G0 X1", "G0 X2", "G0 X3", "G0 X4"];

fn running() -> Rig {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&JOB));
    rig.act("startRun");
    rig
}

/// The ways a job stops without Stop, each from a running job.
fn pauses() -> Vec<(&'static str, Rig)> {
    let mut user = running();
    user.act("pauseRun");
    let mut program = Rig::connected();
    program.machine.load_gcode(&file(&["G21", "M0", "G0 X1"]));
    program.act("startRun");
    program.run_to_quiet();
    program.rx("Maslow Paused");
    let mut tool = Rig::connected();
    tool.machine.load_gcode(&file(&["G21", "T7 M6", "G0 X1"]));
    tool.act("startRun");
    tool.run_to_quiet();
    tool.rx("Tool Change: Please insert tool 7");
    tool.rx("Maslow Paused");
    let mut controller = running();
    controller.rx("Maslow Paused");
    let mut adjust_z = running();
    adjust_z.rx("Message: Please adjust Z-Axis to a depth of -3.00 mm");
    let mut alarm = running();
    alarm.rx("ALARM: The sled is not keeping up with its expected position and has halted.");
    let mut overflow = running();
    overflow.rx("Buffer overflow!");
    let mut lost = running();
    lost.send(Event::PortClosed);
    vec![
        ("Pause", user),
        ("M0", program),
        ("tool change", tool),
        ("Maslow Paused", controller),
        ("adjust Z", adjust_z),
        ("ALARM", alarm),
        ("Buffer overflow", overflow),
        ("lost link", lost),
    ]
}

#[test]
fn every_kind_of_pause_keeps_the_job_and_its_gcode_until_stop() {
    pauses().into_iter().for_each(|(name, mut rig)| {
        let gcode = rig.machine.snapshot().gcode;
        assert!(rig.machine.snapshot().job_in_progress, "{name}");
        assert!(rig.machine.snapshot().gcode_locked, "{name}");
        assert_eq!(alerts(&rig.act("clearGCode")), [GCODE_LOCKED], "{name}");
        let parsed = Gcode::parse("G0 X9\n", &settings(&[])).unwrap();
        let load = Request::LoadGcode { gcode: Box::new(parsed), file: None, origin: Origin::Edit };
        let replies = rig.send(Event::Request { id: 1, request: load });
        assert!(replies.iter().any(|e| matches!(e, Effect::Reply { reply: wc_machine::files::Reply::Refused(_), .. })));
        assert_eq!(rig.machine.snapshot().gcode, gcode, "{name}");
        assert_eq!(rig.machine.snapshot().pause_button, wc_protocol::main::PauseButton::Resume, "{name}");
        rig.act("stopRun");
        assert!(!rig.machine.snapshot().job_in_progress, "{name}");
        assert!(!rig.machine.snapshot().gcode_locked, "{name}");
    });
}

#[test]
fn the_end_of_the_file_ends_the_job_and_the_next_file_runs() {
    let mut rig = running();
    rig.run_to_quiet();
    assert_eq!(flags(&rig), (0, false, 0));
    rig.machine.load_gcode("G0 X9\n");
    rig.act("startRun");
    assert_eq!(flags(&rig).0, 1);
}

#[test]
fn a_controller_pause_lets_jogs_through_and_play_resumes_with_tilde() {
    let mut program = Rig::connected();
    program.machine.load_gcode(&file(&["G21", "G0 X1", "M0", "G0 X2", "G0 X3", "G0 X4"]));
    program.act("startRun");
    program.run_to_quiet();
    program.rx("Maslow Paused");
    assert_eq!(written(&program.act_with("move", json!("right"), json!("10"))), ["~"]);
    assert_eq!(program.run_to_quiet(), ["~", "G91", "G00 X10.0", "G90"]);
    assert!(alerts(&program.act("startRun")).is_empty());
    assert_eq!(flags(&program), (1, true, 3));
    let mut alarm = running();
    alarm.run_to_quiet();
    let mut alarm = Rig::connected();
    alarm.machine.load_gcode(&file(&JOB));
    alarm.act("startRun");
    alarm.rx("ALARM: Position Lost");
    alarm.advance(300);
    alarm.rx("<Idle,MPos:0.00,0.00,0.00,WPos:0,0,0>");
    alarm.clear_written();
    let jog = alarm.act_with("move", json!("right"), json!("10"));
    assert!(alerts(&jog)[0].contains("Press Stop to clear it, or Play"), "{jog:?}");
    assert!(written(&jog).is_empty() && alarm.run_to_quiet().is_empty(), "an alarm refuses jogs");
    assert_eq!(written(&alarm.act("startRun")), ["~"], "Play resumes an alarmed job");
}

#[test]
fn a_user_pause_or_a_paused_controller_refuses_play_and_home_until_stop() {
    let mut paused = running();
    paused.act("pauseRun");
    assert_eq!(alerts(&paused.act("startRun")), [PAUSED]);
    let mut held = running();
    held.rx("Maslow Paused");
    ["defineHome", "resetHomeToCenter"].iter().for_each(|command| {
        let home = held.machine.snapshot().settings.text("Advanced Settings", "homeX");
        assert_eq!(alerts(&held.act_with(command, json!(12), json!(3))), [PAUSED], "{command}");
        assert_eq!(held.machine.snapshot().settings.text("Advanced Settings", "homeX"), home);
    });
    let pid = json!({"kind": "velocity", "motor": "L", "start": 0, "stop": 1, "steps": 1, "version": 1, "kp": 1,
        "ki": 1, "kd": 1});
    let effects = held.send(Event::Edit { id: 1, request: wc_machine::Request::PidTest(pid) });
    assert!(effects.iter().any(|e| matches!(e, Effect::Respond { response, .. } if response.status == 409)));
    held.act("stopRun");
    assert!(alerts(&held.act_with("defineHome", json!(1), json!(0))).is_empty());
}

#[test]
fn a_tool_change_outside_a_job_holds_home_and_resumes_without_starting_the_file() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&JOB));
    rig.send(Event::Request { id: 1, request: Request::SendGcode("T2 M6".into()) });
    assert_eq!(rig.run_to_quiet(), ["T2 M6"]);
    rig.rx("Tool Change: Please insert tool 2");
    rig.rx("Maslow Paused");
    let snapshot = rig.machine.snapshot();
    assert_eq!((snapshot.manual_z_axis_adjust, snapshot.job_in_progress, snapshot.current_tool), (true, false, 2));
    let refusal = alerts(&rig.act_with("defineHome", json!(12), json!(3)));
    assert!(refusal[0].starts_with("Cannot issue command while a tool change waits."));
    assert_eq!(resume(&mut rig), ["~"]);
    assert_eq!(flags(&rig), (0, false, 0));
}

#[test]
fn m0_and_m1_stop_streaming_after_their_line_and_longer_m_codes_do_not() {
    ["M0", "M1", "M00", "M01", "G1 X2 M0", "M1 G1 X2"].iter().for_each(|line| {
        let mut rig = Rig::connected();
        rig.machine.load_gcode(&file(&["G1 X1", line, "G1 X3"]));
        rig.act("startRun");
        assert_eq!(rig.run_to_quiet().len(), 2, "{line}");
        assert_eq!(flags(&rig), (-1, true, 2), "{line}");
    });
    ["M30", "M10", "M100", "G1 X1.0"].iter().for_each(|line| {
        let mut rig = Rig::connected();
        rig.machine.load_gcode(&file(&["G1 X1", line, "G1 X3"]));
        rig.act("startRun");
        assert_eq!(rig.run_to_quiet().len(), 3, "{line}");
    });
}

#[test]
fn resume_after_m0_releases_the_firmware_and_returns_to_the_next_line() {
    let mut rig = Rig::connected();
    let lines = ["G21", "G90", "G0 Z5", "G0 X10 Y10", "G1 Z-3 F100", "G1 X20 F300", "M0", "G1 X30"];
    rig.machine.load_gcode(&file(&lines));
    rig.act("startRun");
    rig.run_to_quiet();
    let effects = rig.rx("Maslow Paused");
    assert!(written(&effects).is_empty(), "the M0 never releases itself");
    assert_eq!(pushed(&effects, "requestedSetting"), [json!({"setting": "pauseButtonSetting", "value": "Resume"})]);
    assert_eq!(flags(&rig), (0, true, 7));
}

/// What Resume writes until the machine is quiet, each line acked at once.
fn resume(rig: &mut Rig) -> Vec<String> {
    rig.clear_written();
    rig.act("resumeRun");
    rig.run_to_quiet()
}

#[test]
fn resume_after_m0_sends_tilde_then_the_restart_then_the_file() {
    let mut rig = Rig::connected();
    let lines = ["G21", "G90", "G0 Z5", "G0 X10 Y10", "G1 Z-3 F100", "G1 X20 F300", "M0", "G1 X30"];
    rig.machine.load_gcode(&file(&lines));
    rig.act("startRun");
    rig.run_to_quiet();
    rig.rx("Maslow Paused");
    let sent = resume(&mut rig);
    let restart = ["~", "G21", "G90", SAFE_Z, "G0 X20.0 Y10.0", "G1 Z-3.0 F100.0", "G1 F300.0", "G90", "G1 X30"];
    assert_eq!(sent, restart);
    assert_eq!(flags(&rig), (0, false, 0));
}

#[test]
fn resume_after_an_m0_outside_a_job_only_releases_the_firmware() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&JOB));
    rig.rx("Maslow Paused");
    assert_eq!(resume(&mut rig), ["~"]);
    assert_eq!(flags(&rig), (0, false, 0));
}

#[test]
fn a_stopped_jobs_pause_never_lowers_a_later_resume() {
    let mut rig = Rig::connected();
    pause_at(&mut rig, &["G21", "G90", "G1 Z-6", "G0 X10 Y10", "M0", "G1 X30"], 4);
    rig.act("stopRun");
    rig.advance(300);
    rig.rx("<Idle,MPos:0.00,0.00,0.00,WPos:0,0,0>");
    rig.run_to_quiet();
    rig.act("moveGcodeGoto");
    rig.act("startRun");
    rig.run_to_quiet();
    rig.rx("Maslow Paused");
    let sent = resume(&mut rig);
    assert!(sent.contains(&"G1 Z-6.0 F12.8".to_string()), "the file's own Z, not a stale pause: {sent:?}");
}

#[test]
fn resume_returns_to_where_the_job_paused_before_lowering_the_bit() {
    let mut rig = Rig::connected();
    pause_at(&mut rig, &["G90", "G0 X10 Y20", "G1 Z-3", "G1 X30 Y40", "G1 X50 Y60"], 4);
    rig.act_with("move", json!("right"), json!("100"));
    rig.run_to_quiet();
    let sent = resume(&mut rig);
    assert_eq!(sent[..7], ["~", "G21", "G90", SAFE_Z, "G0 X30.0 Y40.0", "G1 Z-3.0 F12.8", "G90"]);
}

#[test]
fn resume_goes_to_the_new_line_when_the_index_moved_while_paused() {
    let mut rig = Rig::connected();
    let lines = ["G90", "G0 X10 Y20", "G1 Z-3", "G1 X30 Y40", "G0 Z2", "G0 X50 Y60", "G1 Z-1"];
    pause_at(&mut rig, &lines, 4);
    rig.act_with("moveGcodeIndex", json!("2"), json!(""));
    let sent = resume(&mut rig);
    assert_eq!(sent[..8], ["~", "G21", "G90", SAFE_Z, "G0 X50.0 Y60.0", "G1 Z2.0 F12.8", "G0", "G90"]);
}

#[test]
fn resume_restores_an_absolute_z_and_relative_mode_in_a_g91_file() {
    let mut rig = Rig::connected();
    pause_at(&mut rig, &["G90", "G0 X0 Y0", "G91", "G0 X10 Y20", "G1 Z-3", "G1 X5", "G1 X1"], 5);
    let sent = resume(&mut rig);
    assert_eq!(sent[..7], ["~", "G21", "G90", SAFE_Z, "G0 X10.0 Y20.0", "G1 Z-3.0 F12.8", "G91"]);
}

const XY_UNKNOWN: &str = "Cannot return to this line: the file moves X or Y relatively (G91)";

#[test]
fn resume_and_play_mid_file_are_refused_before_a_g91_file_sets_x_and_y() {
    let mut rig = Rig::connected();
    pause_at(&mut rig, &["G91", "G0 X10 Y20", "G1 Z-3", "G1 X5", "G1 X1"], 3);
    rig.clear_written();
    let refused = alerts(&rig.act("resumeRun"));
    assert!(refused.len() == 1 && refused[0].starts_with(XY_UNKNOWN), "{refused:?}");
    assert!(rig.run_to_quiet().is_empty(), "no `~`, no restart");
    assert_eq!(flags(&rig).0, 2, "still paused");
    rig.act("stopRun");
    rig.run_to_quiet();
    rig.act_with("moveGcodeGoto", json!(3), json!(""));
    rig.clear_written();
    assert!(alerts(&rig.act("startRun"))[0].starts_with(XY_UNKNOWN));
    assert_eq!((rig.run_to_quiet(), rig.machine.snapshot().gcode_index), (Vec::<String>::new(), 3));
}

#[test]
fn resume_restores_the_units_the_job_was_paused_in() {
    let mut rig = Rig::connected();
    pause_at(&mut rig, &["G90", "G0 X10 Y20", "G1 Z-3", "G1 X30 Y40", "G1 X1"], 3);
    let refused = rig.send(Event::UpdateSetting { setting: "toInches".into(), value: json!(1) });
    assert_eq!(alerts(&refused), [wc_machine::files::SETTINGS_LOCKED], "decision 2: no unit switch mid-job");
    assert!(written(&refused).is_empty());
    assert_eq!(rig.machine.snapshot().units(), wc_protocol::main::Units::Mm);
    let sent = resume(&mut rig);
    assert_eq!(sent[..5], ["~", "G21", "G90", SAFE_Z, "G0 X10.0 Y20.0"]);
    assert_eq!(rig.machine.snapshot().units(), wc_protocol::main::Units::Mm);
}

#[test]
fn resume_does_not_repeat_a_tool_change_and_a_second_resume_moves_nothing() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&["T0 M6", "G90", "G0 X10 Y20", "G1 Z-3", "G1 X30 Y40", "G1 X1", "G1 X2"]));
    rig.act("startRun");
    (0..5).for_each(|_| {
        rig.advance(60);
        rig.rx("ok");
    });
    rig.act("pauseRun");
    rig.rx("ok");
    let sent = written(&rig.act("resumeRun"));
    assert!(!sent.iter().any(|line| line.contains("M6")));
    assert!(written(&rig.act("resumeRun")).is_empty(), "already streaming: no lift and replunge");
}

const TOOL_CHANGE_FILE: [&str; 13] = [
    "G21",
    "G90",
    "G0 Z5",
    "G0 X10 Y10",
    "G1 Z-3",
    "G1 X20",
    "G0 Z5",
    "G0 X0 Y0",
    "T2 M6",
    "G0 X40 Y40",
    "G0 Z5",
    "G1 Z-2",
    "G1 X50",
];

fn lowered(sent: &[String]) -> Vec<String> {
    sent.iter().filter(|l| (l.starts_with("G0 Z") || l.starts_with("G1 Z")) && *l != SAFE_Z).cloned().collect()
}

fn start_at(rig: &mut Rig, lines: &[&str], index: i64) -> Vec<String> {
    rig.machine.load_gcode(&file(lines));
    rig.act_with("moveGcodeGoto", json!(index), json!(""));
    rig.clear_written();
    rig.act("startRun");
    rig.run_to_quiet()
}

#[test]
fn a_restart_lowers_the_bit_only_to_a_z_the_file_set_for_the_current_tool() {
    let mut before_z = Rig::connected();
    assert!(lowered(&start_at(&mut before_z, &["G21", "G90", "G0 X10 Y10", "G1 X20 Y20"], 3)).is_empty());
    let mut after_change = Rig::connected();
    let sent = start_at(&mut after_change, &TOOL_CHANGE_FILE, 9);
    assert!(sent.contains(&"T2 M6".to_string()));
    assert_eq!(flags(&after_change).0, -1, "the firmware pauses on the restart's M6");
    assert!(lowered(&sent).is_empty(), "nothing queued behind the M6: {sent:?}");
}

#[test]
fn play_during_a_tool_change_resumes_where_it_paused_at_safe_height() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&TOOL_CHANGE_FILE));
    rig.act("startRun");
    rig.run_to_quiet();
    rig.rx("Tool Change: Please insert tool 2");
    rig.rx("Maslow Paused");
    assert_eq!(flags(&rig), (0, true, 9));
    rig.clear_written();
    rig.act("startRun");
    let sent = rig.run_to_quiet();
    assert_eq!(sent[3..5], [SAFE_Z, "G0 X0.0 Y0.0"]);
    assert_eq!(lowered(&sent[..7]), Vec::<String>::new());
    assert_eq!(lowered(&sent), ["G0 Z5", "G1 Z-2"], "the file's own moves after the return");
}

#[test]
fn a_restart_reaches_the_firmware_paused_by_its_own_m6_and_resumes_after_the_change() {
    let mut rig = Rig::connected();
    let sent = start_at(&mut rig, &TOOL_CHANGE_FILE, 12);
    assert_eq!(sent, ["G21", "G90", SAFE_Z, "G0 X40.0 Y40.0", "T2 M6"]);
    rig.rx("Tool Change: Please insert tool 2");
    rig.rx("Maslow Paused");
    let sent = resume(&mut rig);
    assert_eq!(sent, ["~", "G21", "G90", SAFE_Z, "G0 X40.0 Y40.0", "G1 Z-2.0 F12.8", "G90", "G1 X50"]);
}

#[test]
fn stop_forgets_the_pause_the_firmware_and_the_tool_it_never_reached() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&["G21", "T2 M6", "G1 X2"]));
    rig.act("startRun");
    rig.run_to_quiet();
    assert_eq!(rig.machine.snapshot().current_tool, 2);
    let effects = rig.act("stopRun");
    assert_eq!(pushed(&effects, "requestedSetting"), [json!({"setting": "pauseButtonSetting", "value": "Pause"})]);
    assert!(pushed(&effects, "clearAlarm").len() == 1);
    assert_eq!(rig.machine.snapshot().current_tool, 0, "the ! flushed the M6");
    let mut confirmed = Rig::connected();
    confirmed.machine.load_gcode(&file(&["G21", "T2 M6", "G1 X2"]));
    confirmed.act("startRun");
    confirmed.run_to_quiet();
    confirmed.rx("Tool Change: Please insert tool 2");
    confirmed.rx("Maslow Paused");
    confirmed.act("stopRun");
    let snapshot = confirmed.machine.snapshot();
    assert_eq!((snapshot.current_tool, snapshot.manual_z_axis_adjust, snapshot.job_in_progress), (2, false, false));
}

#[test]
fn stop_z_ends_a_held_job_and_says_so() {
    let mut rig = running();
    rig.rx("Maslow Paused");
    let effects = rig.act("stopZ");
    assert_eq!(written(&effects), ["!"]);
    assert_eq!(pushed(&effects, "requestedSetting")[0]["value"], "Pause");
    assert!(!rig.machine.snapshot().job_in_progress);
    let mut idle = Rig::connected();
    assert!(pushed(&idle.act("stopZ"), "requestedSetting").is_empty());
}

#[test]
fn the_pause_button_and_the_status_follow_the_job() {
    let mut rig = running();
    let effects = rig.act("pauseRun");
    assert_eq!(pushed(&effects, "statusMessage")[0]["uploadFlag"], 2);
    assert_eq!(pushed(&effects, "requestedSetting")[0]["value"], "Resume");
    let answer = rig.send(Event::SettingRequest { section: "None".into(), setting: "pauseButtonSetting".into() });
    assert_eq!(pushed(&answer, "requestedSetting")[0]["value"], "Resume");
    assert!(pushed(&rig.act("pauseRun"), "requestedSetting").is_empty(), "a second Pause changes nothing");
    let resumed = rig.act("resumeRun");
    assert_eq!(pushed(&resumed, "requestedSetting").last().unwrap()["value"], "Pause");
}

#[test]
fn a_start_mid_file_that_cannot_read_the_file_alerts_and_starts_nothing() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&["G0 X.", "G0 X2", "G0 X3"]));
    rig.act_with("moveGcodeGoto", json!(2), json!(""));
    assert_eq!(alerts(&rig.act("startRun")), ["Error with starting run."]);
    assert_eq!(flags(&rig), (0, false, 0));
    let mut no_home = Rig::connected_with(settings(&[("Advanced Settings", "homeX", json!("left"))]));
    no_home.machine.load_gcode(&file(&JOB));
    no_home.act("startRun");
    no_home.rx("Maslow Paused");
    assert_eq!(alerts(&no_home.act("resumeRun")), ["Error with resuming run."]);
}

#[test]
fn a_play_led_a_pause_led_and_a_stop_led_follow_the_job() {
    let leds = |effects: &[Effect]| {
        effects.iter().rev().find_map(|e| match e {
            Effect::Leds(leds) => Some((leds.play, leds.pause, leds.stop)),
            _ => None,
        })
    };
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&JOB));
    assert_eq!(leds(&rig.act("startRun")), Some((true, false, false)));
    assert_eq!(leds(&rig.act("pauseRun")), Some((false, true, false)));
    assert_eq!(leds(&rig.act("resumeRun")), Some((false, false, false)), "Python only turns Pause off");
    assert_eq!(leds(&rig.act("stopRun")), Some((false, false, true)));
    assert_eq!(leds(&rig.act("stopRun")), None, "unchanged LEDs are not sent again");
}

#[test]
fn a_moved_index_shows_where_the_job_will_restart() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&["G0 X1 Y2", "G0 X3", "G0 X4"]));
    let effects = rig.act_with("moveGcodeGoto", json!(2), json!(""));
    let position: Vec<Value> = pushed(&effects, "gcodePositionMessage");
    assert_eq!(position[0]["xval"], 3.0);
}

/// Wart 14: Play waits for a gcode load being built, or the old file would cut in its place.
#[test]
fn play_is_refused_while_a_load_builds_and_a_failed_load_ends_every_spinner() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&JOB));
    let started = rig.send(Event::Request { id: 1, request: Request::LoadStarting });
    assert_eq!(pushed(&started, "showFPSpinner"), [json!(0)]);
    let refused = rig.send(Event::Button(wc_machine::gpio::Button::Play));
    assert_eq!(alerts(&refused), ["Gcode is still loading. Start the job when the preview appears."]);
    let unreadable = Request::Problem(wc_machine::files::Problem::GcodeUnreadable);
    let failed = rig.send(Event::Request { id: 2, request: unreadable });
    assert!(rig::commands(&failed).contains(&"gcodeUpdateCompressed".to_string()));
    assert!(alerts(&rig.act("startRun")).is_empty());
    let mut loaded = Rig::connected();
    loaded.send(Event::Request { id: 1, request: Request::LoadStarting });
    let gcode = Box::new(Gcode::parse("G0 X9\n", &settings(&[])).unwrap());
    loaded.send(Event::Request { id: 2, request: Request::LoadGcode { gcode, file: None, origin: Origin::Open } });
    assert!(alerts(&loaded.act("startRun")).is_empty());
    assert_eq!(flags(&loaded).0, 1);
}
