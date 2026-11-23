//! Parking: a file that finishes normally parks the sled at the park position, in machine coordinates and the units
//! the controller is in; an abnormal end never parks; Park and Set Park Here pass the same gates as the moves.

mod rig;

use rig::{Rig, alerts, pushed, settings, written};
use serde_json::{Value, json};
use wc_machine::{Effect, Event, Request};

const PARK: [(&str, &str, &str); 2] = [("Maslow Settings", "parkX", "-300"), ("Maslow Settings", "parkY", "200.5")];
const PARKED_MM: [&str; 4] = ["G21", "G90", "G00 Z5.0", "G00 X-300.0 Y200.5"];

fn parked_with(changes: &[(&str, &str, Value)]) -> Rig {
    let park = PARK.iter().map(|(section, key, value)| (*section, *key, json!(value)));
    let all: Vec<(&str, &str, Value)> = park.chain(changes.iter().cloned()).collect();
    let mut rig = Rig::connected_with(settings(&all));
    rig.clear_written();
    rig
}

fn parked() -> Rig {
    parked_with(&[])
}

fn play(rig: &mut Rig, file: &str) -> Vec<Effect> {
    rig.machine.load_gcode(file);
    rig.act("startRun")
}

/// Plays `file` and acks every line but the last, which is in flight when this returns.
fn all_but_the_last_acked(rig: &mut Rig, file: &str) {
    play(rig, file.trim_end());
    let lines = file.lines().count();
    (1..lines).for_each(|_| {
        rig.advance(100);
        rig.rx("ok");
    });
    rig.advance(100);
    let snapshot = rig.machine.snapshot();
    assert!(!snapshot.job_in_progress, "the last line went out: {} of {}", snapshot.gcode_index, snapshot.gcode_count);
}

fn logged(effects: &[Effect], label: &str) -> Vec<String> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Log(text) if text.starts_with(label) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_file_that_finishes_parks_at_safe_height_once_its_last_line_is_answered() {
    let mut rig = parked();
    all_but_the_last_acked(&mut rig, "G21\nG0 Z3\nG1 X10 F100\n");
    let mut sent = rig.run_to_quiet();
    assert_eq!(sent.drain(..3).collect::<Vec<_>>(), ["G21", "G0 Z3", "G1 X10 F100"]);
    assert_eq!(sent, PARKED_MM, "Z rises to the safe height, then the sled rapids there and stays up");
    let effects = rig.take_effects();
    assert_eq!(logged(&effects, "PARK"), ["PARK START trigger=file x_mm=-300.0 y_mm=200.5 units=MM x=-300.0 y=200.5"]);
}

#[test]
fn a_bit_above_the_safe_height_is_never_lowered_to_park() {
    let mut rig = parked();
    play(&mut rig, "G21\nG0 Z12\n");
    assert_eq!(rig.run_to_quiet(), ["G21", "G0 Z12", "G21", "G90", "G00 Z12.0", "G00 X-300.0 Y200.5"]);
}

#[test]
fn a_millimetre_file_on_an_inch_machine_parks_in_millimetres_and_an_inch_file_in_inches() {
    let inches = [("Computed Settings", "units", json!("INCHES"))];
    let mut rig = parked_with(&inches);
    play(&mut rig, "G21\nG1 X10 F100\n");
    assert_eq!(rig.run_to_quiet(), ["G21", "G1 X10 F100", "G21", "G90", "G00 Z5.0", "G00 X-300.0 Y200.5"]);

    let mut rig = parked_with(&inches);
    play(&mut rig, "G20\nG1 X1 F10\n");
    let sent = rig.run_to_quiet();
    assert_eq!(sent[2..], ["G20", "G90", "G00 Z0.197", "G00 X-11.811 Y7.8937"], "{sent:?}");
}

#[test]
fn moving_home_does_not_move_the_park_position() {
    let mut rig = parked();
    rig.act_with("defineHome", json!(10), json!(-5));
    assert_eq!(rig.machine.snapshot().state_fields()["home"], json!({"x": 254.0, "y": -127.0}));
    rig.act("park");
    assert_eq!(rig.run_to_quiet(), PARKED_MM);
    play(&mut rig, "G21\nG1 X10 F100\n");
    let sent = rig.run_to_quiet();
    assert_eq!(sent[1], "G1 X264 F100", "the file is shifted to home");
    assert_eq!(sent[2..], PARKED_MM, "the park position is not");
}

#[test]
fn no_park_position_means_no_park_and_a_refused_button() {
    let mut rig = Rig::connected();
    rig.clear_written();
    play(&mut rig, "G21\nG1 X10 F100\n");
    assert_eq!(rig.run_to_quiet(), ["G21", "G1 X10 F100"]);
    assert_eq!(alerts(&rig.act("park")), [wc_machine::park::NOT_SET]);
    assert!(rig.run_to_quiet().is_empty());
}

#[test]
fn stop_after_the_last_line_went_out_cancels_the_park() {
    let mut rig = parked();
    all_but_the_last_acked(&mut rig, "G21\nG1 X10 F100\n");
    rig.clear_written();
    rig.act("stopRun");
    rig.advance(300);
    rig.rx("<Idle,MPos:0.00,0.00,0.00,WPos:0.000,0.000,0.000>");
    assert_eq!(rig.run_to_quiet(), ["!"]);
}

#[test]
fn pause_after_the_last_line_went_out_cancels_the_park() {
    let mut rig = parked();
    all_but_the_last_acked(&mut rig, "G21\nG1 X10 F100\n");
    rig.act("pauseRun");
    assert_eq!(rig.run_to_quiet()[2..], [] as [&str; 0], "the last line is answered, and nothing follows");
}

#[test]
fn a_file_stopped_or_held_by_an_alarm_never_parks() {
    let mut rig = parked();
    play(&mut rig, "G21\nG1 X10 F100\nG1 X20\n");
    rig.advance(100);
    rig.act("stopRun");
    rig.advance(300);
    rig.rx("<Idle,MPos:0.00,0.00,0.00,WPos:0.000,0.000,0.000>");
    assert!(!rig.run_to_quiet().iter().any(|line| line.starts_with("G00 X")));

    let mut rig = parked();
    all_but_the_last_acked(&mut rig, "G21\nG1 X10 F100\n");
    rig.clear_written();
    rig.rx("ALARM: The sled is not keeping up with its expected position and has halted.");
    rig.advance(300);
    rig.rx("<Idle,MPos:0.00,0.00,0.00,WPos:0.000,0.000,0.000>");
    assert!(rig.run_to_quiet().is_empty());
    assert!(alerts(&rig.act("park"))[0].contains("alarm"));
}

#[test]
fn a_lost_link_or_a_controller_reset_before_the_last_ack_never_parks() {
    let mut rig = parked();
    all_but_the_last_acked(&mut rig, "G21\nG1 X10 F100\n");
    rig.send(Event::PortClosed);
    rig.boot_on("51.27");
    rig.clear_written();
    assert!(rig.run_to_quiet().is_empty());

    let mut rig = parked();
    all_but_the_last_acked(&mut rig, "G21\nG1 X10 F100\n");
    rig.rx("PCB v1.6 TLE9201  Detected");
    let sent = rig.answer_until_quiet("51.27");
    rig.rx("<Idle,MPos:0.00,0.00,0.00,WPos:0.000,0.000,0.000>");
    assert!(!sent.iter().chain(&rig.run_to_quiet()).any(|line| line.starts_with("G00 X")), "{sent:?}");
}

#[test]
fn a_run_with_a_refused_line_or_a_controller_error_parks_once_it_reaches_the_end() {
    let mut rig = parked();
    play(&mut rig, "G21\nG1 X10 F100 (unclosed\nG1 X20\n");
    let sent = rig.run_to_quiet();
    assert_eq!(sent, ["G21"], "the job paused before the refused line");
    rig.act("moveGcodeIndex");
    rig.act_with("moveGcodeIndex", json!(1), json!(""));
    rig.act("resumeRun");
    let sent = rig.run_to_quiet();
    assert!(sent.ends_with(&PARKED_MM.map(String::from)), "{sent:?}");

    let mut rig = parked();
    play(&mut rig, "G21\nG1 X10 F100\nG1 X20\n");
    rig.advance(100);
    rig.rx("error: Bad number format");
    let sent = rig.run_to_quiet();
    assert_eq!(sent[3..], PARKED_MM, "the job went on to its end, so it parks: {sent:?}");

    let mut rig = parked();
    all_but_the_last_acked(&mut rig, "G21\nG1 X10 F100\n");
    rig.rx("error: Bad number format");
    assert_eq!(rig.run_to_quiet()[2..], [] as [&str; 0], "an error for the last line cancels the park");
}

#[test]
fn a_command_sent_before_the_last_line_is_answered_cancels_the_park() {
    let mut rig = parked();
    all_but_the_last_acked(&mut rig, "G21\nG1 X10 F100\n");
    rig.act_with("move", json!("up"), json!("1"));
    let sent = rig.run_to_quiet();
    assert_eq!(sent[sent.len() - 3..], ["G91", "G00 Y1.0", "G90"]);
    assert!(!sent.contains(&"G00 X-300.0 Y200.5".to_string()));

    let mut rig = parked();
    all_but_the_last_acked(&mut rig, "G21\nG1 X10 F100\n");
    rig.send(Event::Request { id: 1, request: wc_machine::files::Request::SendGcode("G0 X1".into()) });
    assert_eq!(rig.run_to_quiet()[2..], ["G0 X1"]);
}

#[test]
fn an_unsafe_safe_height_skips_the_park_with_an_alert() {
    let mut rig = parked_with(&[("Maslow Settings", "zAxisSafeHeight", json!("0"))]);
    play(&mut rig, "G21\nG1 X10 F100\n");
    assert_eq!(rig.run_to_quiet(), ["G21", "G1 X10 F100"]);
    let effects = rig.take_effects();
    let alert = alerts(&effects).into_iter().find(|alert| alert.contains("did not park")).expect("an alert");
    assert!(alert.contains("zAxisSafeHeight"), "{alert}");
    assert!(logged(&effects, "PARK SKIPPED trigger=file reason=Set the Z-axis safe height").len() == 1);
}

#[test]
fn park_is_refused_during_a_job_at_a_tool_change_and_off_the_bed() {
    let mut rig = parked();
    play(&mut rig, "G21\nG1 X10 F100\nG1 X20\n");
    assert_eq!(alerts(&rig.act("park")), ["Cannot issue command while sending gcode."]);
    rig.act("pauseRun");
    assert!(alerts(&rig.act("park"))[0].starts_with("Cannot issue command while paused"));
    rig.act("stopRun");

    let mut rig = parked();
    rig.send(Event::Request { id: 1, request: wc_machine::files::Request::SendGcode("T2 M6".into()) });
    rig.run_to_quiet();
    rig.rx("Tool Change: Please insert tool 2");
    rig.rx("Maslow Paused");
    assert_eq!(alerts(&rig.act("park")), [wc_machine::park::TOOL_CHANGE]);

    let mut rig = parked_with(&[("Maslow Settings", "parkX", json!("2000"))]);
    assert!(alerts(&rig.act("park"))[0].contains("off the"));
    assert!(rig.run_to_quiet().is_empty());

    let mut rig =
        Rig::with(settings(&[("Maslow Settings", "parkX", json!("1")), ("Maslow Settings", "parkY", json!("2"))]));
    assert_eq!(alerts(&rig.act("park")), ["The controller is not connected."]);
}

#[test]
fn park_waits_for_a_missed_restart() {
    let mut rig = parked();
    let body = json!({"values": {"chainOverSprocket": "Top"}});
    rig.send(Event::Edit { id: 1, request: Request::UpdateSettings { section: "advanced".into(), body } });
    assert_eq!(rig.run_to_quiet(), ["$38=1"]);
    rig.open();
    rig.advance(3100);
    rig.answer_until_quiet("51.27");
    assert!(alerts(&rig.act("park"))[0].contains("did not restart"));
    assert!(alerts(&rig.act("definePark"))[0].contains("did not restart"));
}

#[test]
fn set_park_here_stores_the_reported_sled_in_millimetres_and_tells_every_client() {
    let mut rig = Rig::connected_with(settings(&[("Computed Settings", "units", json!("INCHES"))]));
    rig.act("definePark");
    rig.rx("<Idle,MPos:-4.86,2.00,0.00,WPos:0.000,0.000,0.000>");
    let effects = rig.act("definePark");
    assert_eq!(pushed(&effects, "parkPosition"), [json!({"position": {"x": -123.444, "y": 50.8}})]);
    let stored = rig.machine.snapshot().settings;
    assert_eq!(stored.value("Maslow Settings", "parkX"), Some(&json!("-123.444")));
    assert_eq!(logged(&effects, "PARK SET"), ["PARK SET x_mm=-123.444 y_mm=50.8"]);
    assert!(rig.run_to_quiet().is_empty(), "nothing goes to the controller");
}

#[test]
fn set_park_here_waits_for_a_report_of_a_still_sled() {
    let mut rig = Rig::connected();
    rig.act_with("move", json!("up"), json!("1"));
    assert_eq!(alerts(&rig.act("definePark")), [wc_machine::park::UNPLACED]);
    play(&mut rig, "G21\nG1 X10 F100\n");
    assert!(alerts(&rig.act("definePark"))[0].starts_with("Cannot issue command while sending"));
}

#[test]
fn a_park_position_saved_in_the_settings_is_checked_and_pushed() {
    let mut rig = Rig::connected();
    let save = |rig: &mut Rig, values: Value| {
        let request = Request::UpdateSettings { section: "maslow".into(), body: json!({ "values": values }) };
        rig.send(Event::Edit { id: 1, request })
    };
    let effects = save(&mut rig, json!({"parkX": "12.5", "parkY": "-40"}));
    assert_eq!(pushed(&effects, "parkPosition"), [json!({"position": {"x": 12.5, "y": -40.0}})]);
    let effects = save(&mut rig, json!({"parkX": "wide"}));
    let response = effects.iter().find_map(|effect| match effect {
        Effect::Respond { response, .. } => Some(response.clone()),
        _ => None,
    });
    assert_eq!(response.unwrap().body["errors"]["parkX"], "Must be a number");
    let effects = save(&mut rig, json!({"parkX": " "}));
    assert_eq!(pushed(&effects, "parkPosition"), [json!({"position": null})]);
}

fn parks_for_tool_changes(changes: &[(&str, &str, Value)]) -> Rig {
    let on = [("Maslow Settings", "parkForToolChange", json!(1))];
    let mut rig = parked_with(&[&on[..], changes].concat());
    rig.rx("<Idle,MPos:0.00,0.00,0.00,WPos:0.000,0.000,0.000>");
    rig.clear_written();
    rig
}

const TOOL_CHANGE_FILE: &str = "G21\nG90\nG0 Z5\nG0 X10 Y10\nG1 Z-3 F100\nG1 X20 F600\nG0 Z3\nT2 M6\nG0 X40 Y40\n";

/// Streams `TOOL_CHANGE_FILE` to its M6 and has the firmware prompt for tool 2.
fn at_the_tool_change(rig: &mut Rig) -> Vec<String> {
    play(rig, TOOL_CHANGE_FILE);
    let mut sent = rig.run_to_quiet();
    rig.rx("Tool Change: Please insert tool 2");
    rig.rx("Maslow Paused");
    sent.extend(rig.run_to_quiet());
    sent
}

#[test]
fn a_tool_change_parks_and_resume_returns_to_the_work_at_safe_height() {
    let mut rig = parks_for_tool_changes(&[("Advanced Settings", "homeX", json!("100"))]);
    let sent = at_the_tool_change(&mut rig);
    let parked = ["~", "G21", "G90", "G00 Z5.0", "G00 X-300.0 Y200.5"];
    assert_eq!(sent[sent.len() - 5..], parked, "{sent:?}");
    assert!(logged(&rig.take_effects(), "PARK START")[0].starts_with("PARK START trigger=tool_change"));
    let snapshot = rig.machine.snapshot();
    assert_eq!((snapshot.upload_flag, snapshot.manual_z_axis_adjust), (0, true), "the job waits at the prompt");

    rig.act("resumeRun");
    let sent = rig.run_to_quiet();
    let back = ["~", "G21", "G90", "G0 Z5.0", "G0 X120.0 Y10.0", "G0 F600.0", "G90", "G0 X140 Y40"];
    assert_eq!(sent[..8], back, "back over the work, shifted by home, at safe height before the file goes on");
    assert_eq!(sent[8..], PARKED_MM, "and the file's end parks");
}

#[test]
fn a_tool_change_on_an_inch_machine_parks_in_inches() {
    let mut rig = parks_for_tool_changes(&[("Computed Settings", "units", json!("INCHES"))]);
    play(&mut rig, "G20\nG0 Z0.2\nG0 X1 Y1\nT2 M6\nG0 X2\n");
    rig.run_to_quiet();
    rig.rx("Tool Change: Please insert tool 2");
    rig.rx("Maslow Paused");
    assert_eq!(rig.run_to_quiet(), ["~", "G20", "G90", "G00 Z0.200", "G00 X-11.811 Y7.8937"]);
}

#[test]
fn with_the_setting_off_a_tool_change_waits_where_it_is() {
    let mut rig = parked();
    rig.rx("<Idle,MPos:0.00,0.00,0.00,WPos:0.000,0.000,0.000>");
    let sent = at_the_tool_change(&mut rig);
    assert_eq!(sent.last().map(String::as_str), Some("T2 M6"), "{sent:?}");
}

#[test]
fn a_tool_change_under_an_alarm_or_without_a_park_position_shows_the_prompt_and_says_why() {
    let mut rig = parks_for_tool_changes(&[("Maslow Settings", "parkX", json!(""))]);
    play(&mut rig, TOOL_CHANGE_FILE);
    rig.run_to_quiet();
    rig.rx("Tool Change: Please insert tool 2");
    let effects = rig.rx("Maslow Paused");
    assert_eq!(alerts(&effects), [format!("The sled did not park for the tool change. {}", wc_machine::park::NOT_SET)]);
    assert_eq!(logged(&effects, "PARK SKIPPED").len(), 1);
    assert!(rig.run_to_quiet().is_empty(), "the firmware stays paused, nothing moves");

    let mut rig = parks_for_tool_changes(&[]);
    play(&mut rig, TOOL_CHANGE_FILE);
    rig.run_to_quiet();
    rig.rx("ALARM: The sled is not keeping up with its expected position and has halted.");
    rig.rx("Tool Change: Please insert tool 2");
    let effects = rig.rx("Maslow Paused");
    assert!(alerts(&effects).iter().any(|alert| alert.starts_with("The sled did not park")), "{:?}", alerts(&effects));
    assert!(!rig.run_to_quiet().iter().any(|line| line.contains("X-300")));
}

#[test]
fn stop_at_a_parked_tool_change_leaves_z_up() {
    let mut rig = parks_for_tool_changes(&[]);
    at_the_tool_change(&mut rig);
    rig.act("stopRun");
    rig.advance(300);
    rig.rx("<Idle,MPos:-300.00,200.50,5.00,WPos:0.000,0.000,0.000>");
    let sent = rig.run_to_quiet();
    assert!(sent.iter().all(|line| !line.starts_with("G00 Z") && !line.starts_with("G0 Z")), "{sent:?}");
    assert!(!rig.machine.snapshot().job_in_progress);
}

/// Streams `file`, answering each line before the M6; the M6 and what follows wait for the firmware's `~`.
fn to_an_unanswered_tool_change(rig: &mut Rig, file: &str) -> Vec<String> {
    let mut pending = written(&play(rig, file));
    let mut sent = Vec::new();
    loop {
        if pending.is_empty() {
            pending = written(&rig.advance(100));
        }
        if pending.is_empty() {
            return sent;
        }
        let line = pending.remove(0);
        if !line.contains("M6") && !sent.iter().any(|sent: &String| sent.contains("M6")) {
            pending.extend(written(&rig.rx("ok")));
        }
        sent.push(line);
    }
}

fn paused_for_tool_2(rig: &mut Rig) -> Vec<Effect> {
    rig.rx("Tool Change: Please insert tool 2");
    rig.rx("Maslow Paused")
}

#[test]
fn buffered_streaming_stops_at_the_m6_so_the_tilde_runs_only_the_park() {
    let mut rig = parks_for_tool_changes(&[("Maslow Settings", "bufferOn", json!(1))]);
    let sent = to_an_unanswered_tool_change(&mut rig, TOOL_CHANGE_FILE);
    assert_eq!(sent.last().map(String::as_str), Some("T2 M6"), "nothing after the M6 went out: {sent:?}");
    let effects = paused_for_tool_2(&mut rig);
    assert_eq!(written(&effects)[0], "~");
    assert!(logged(&rig.take_effects(), "PARK START").len() == 1);
}

#[test]
fn a_jog_queued_behind_the_m6_keeps_the_firmware_paused_and_skips_the_park() {
    let mut rig = parks_for_tool_changes(&[("Maslow Settings", "bufferOn", json!(1))]);
    to_an_unanswered_tool_change(&mut rig, TOOL_CHANGE_FILE);
    let jog = rig.act_with("move", json!("right"), json!("10"));
    assert!([written(&jog), written(&rig.advance(100))].concat().is_empty(), "the jog waits for the M6's ok");
    let effects = paused_for_tool_2(&mut rig);
    assert!(written(&effects).is_empty(), "no ~, so the jog and the park cannot run now: {:?}", written(&effects));
    assert_eq!(logged(&effects, "PARK SKIPPED").len(), 1);
    assert!(alerts(&effects)[0].contains("would run before the park"), "{:?}", alerts(&effects));
}

#[test]
fn job_lines_buffered_behind_an_unexpected_tool_change_never_get_a_tilde() {
    let mut rig = parks_for_tool_changes(&[("Maslow Settings", "bufferOn", json!(1))]);
    // WebControl takes tool 0 to be fitted, so `T0 M6` streams on; a firmware that disagrees still prompts.
    let file = format!("G21\nT0 M6\nG1 Z-3 F100\n{}", "G1 X40 Y40 F600\nG1 X10 Y10 F600\n".repeat(8));
    let sent = to_an_unanswered_tool_change(&mut rig, &file);
    assert!(sent[2..].starts_with(&["G1 Z-3 F100".to_string()]), "{sent:?}");
    assert!(rig.machine.snapshot().job_in_progress, "the buffer is full, the job not yet streamed");
    rig.rx("Tool Change: Please insert tool 0");
    let effects = rig.rx("Maslow Paused");
    assert!(written(&effects).is_empty(), "a ~ would cut with the old bit: {:?}", written(&effects));
    assert_eq!(logged(&effects, "PARK SKIPPED").len(), 1);
}

#[test]
fn a_tool_change_line_that_moves_after_its_m6_does_not_park() {
    let mut rig = parks_for_tool_changes(&[]);
    to_an_unanswered_tool_change(&mut rig, "G21\nG0 Z5\nT2 M6 G0 X40 Y40\nG1 Z-3 F100\n");
    let effects = paused_for_tool_2(&mut rig);
    assert!(written(&effects).is_empty(), "{:?}", written(&effects));
    assert_eq!(logged(&effects, "PARK SKIPPED").len(), 1);
}
