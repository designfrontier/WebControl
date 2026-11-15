//! Streaming, ported from test_serial_streaming and test_message_processor, and the checklist's air cut run against
//! `wc-sim`: the home shift, the Z target, refused lines, halts, buffered flow control and a whole job.

mod rig;

use rig::sim::SimRig;
use rig::{Rig, alerts, pushed, settings, written};
use serde_json::json;
use wc_machine::Event;
use wc_machine::files::{Origin, Request};

fn file(lines: &[&str]) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
}

fn stream(rig: &mut Rig, lines: &[&str]) -> Vec<String> {
    rig.machine.load_gcode(&file(lines));
    rig.clear_written();
    rig.act("startRun");
    rig.run_to_quiet()
}

#[test]
fn the_home_shift_applies_to_absolute_moves_only() {
    let mut rig = Rig::connected_with(settings(&[("Advanced Settings", "homeX", json!("10"))]));
    let sent = stream(&mut rig, &["G91 G0 X1", "G90 G0 X1", "G91", "G0 X2", "G90", "G0 X3"]);
    assert_eq!(sent, ["G91 G0 X1", "G90 G0 X11", "G91", "G0 X2", "G90", "G0 X13"]);
}

#[test]
fn the_home_shift_can_push_a_line_over_the_limit() {
    let mut rig = Rig::connected_with(settings(&[("Advanced Settings", "homeX", json!("1000.123456"))]));
    let line = format!("G1 X1 {}", "Y1 ".repeat(38).trim_end());
    let effects = {
        rig.machine.load_gcode(&file(&[&line]));
        rig.act("startRun")
    };
    assert!(written(&effects).is_empty());
    assert!(pushed(&effects, "activateModal")[0]["title"] == "Line Too Long");
}

#[test]
fn a_line_that_just_fits_the_controller_buffer_goes_out() {
    let mut rig = Rig::connected();
    let line = format!("G1 X10{}", " Y1".repeat(39));
    assert_eq!(line.len() + 3, 126, "the loader's space, the home shift's space and the newline");
    assert_eq!(stream(&mut rig, &[&line]), [line]);
}

#[test]
fn resuming_after_a_refused_line_stops_at_that_line_again() {
    let mut rig = Rig::connected();
    stream(&mut rig, &["G0 X1", "G0 (open", "G0 X3"]);
    assert_eq!(rig.machine.snapshot().upload_flag, 2);
    rig.clear_written();
    let effects = rig.act("resumeRun");
    let mut later = rig.run_to_quiet();
    later.extend(written(&rig.advance(200)));
    assert!(!later.iter().any(|line| line.contains("open")), "{later:?}");
    let snapshot = rig.machine.snapshot();
    assert_eq!((snapshot.upload_flag, snapshot.gcode_index), (2, 1));
    assert!(alerts(&effects).is_empty());
}

#[test]
fn the_z_target_is_absolute_through_relative_moves() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&["G90", "G1 Z-2", "G91", "G1 Z-1", "G90", "G1 X5", "G1 X6"]));
    rig.act("startRun");
    (0..6).for_each(|_| {
        rig.advance(60);
        rig.rx("ok");
    });
    rig.act("pauseRun");
    rig.rx("ok");
    rig.clear_written();
    rig.act("resumeRun");
    assert!(rig.run_to_quiet().contains(&"G1 Z-3.0 F12.8".to_string()));
}

#[test]
fn a_halt_outside_a_job_leaves_the_index_alone() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&["G0 X1", "G0 X2", "G0 X3"]));
    rig.act_with("moveGcodeGoto", json!(2), json!(""));
    rig.act_with("move", json!("right"), json!("10"));
    rig.rx("ALARM: The sled is not keeping up");
    let snapshot = rig.machine.snapshot();
    assert_eq!((snapshot.gcode_index, snapshot.job_in_progress), (2, false));
}

#[test]
fn a_garbled_line_never_stops_an_alarm_halting_the_job() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&["G0 X1", "G0 X2"]));
    rig.act("startRun");
    rig.rx("<Idle,MPos:x,y,z>");
    rig.rx("[PE:");
    rig.rx("ALARM: Position Lost");
    assert_eq!(rig.machine.snapshot().upload_flag, 0);
}

#[test]
fn a_lost_link_mid_cut_pauses_and_resume_after_the_reconnect_restarts_at_the_lost_line() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&["G21", "G90", "G0 X1", "G1 Z-1", "G1 X2", "G1 X3"]));
    rig.act("startRun");
    (0..3).for_each(|_| {
        rig.advance(60);
        rig.rx("ok");
    });
    let effects = rig.advance(5100);
    assert!(effects.contains(&wc_machine::Effect::ClosePort));
    let snapshot = rig.machine.snapshot();
    assert_eq!((snapshot.upload_flag, snapshot.gcode_index), (2, 3), "rewound to the line in flight");
    rig.open();
    ["PCB v1.6 TLE9201  Detected", "Grbl v1.00", "ready", "ok"].iter().for_each(|line| {
        rig.rx(line);
    });
    rig.run_to_quiet();
    assert!(rig.machine.snapshot().job_in_progress, "the reboot a reconnect causes keeps a paused job");
    rig.clear_written();
    rig.act("resumeRun");
    let sent = rig.run_to_quiet();
    assert_eq!(sent[..5], ["~", "G21", "G90", "G0 Z5.0", "G0 X1.0 Y0.0"]);
    assert!(sent.contains(&"G1 Z-1".to_string()), "the lost line goes out again: {sent:?}");
}

#[test]
fn a_controller_restart_mid_stream_ends_the_job() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&["G0 X1", "G0 X2"]));
    rig.act("startRun");
    let effects = rig.rx("PCB v1.6 TLE9201  Detected");
    assert!(alerts(&effects)[0].contains("restarted"));
    assert!(!rig.machine.snapshot().job_in_progress);
}

#[test]
fn a_file_switching_units_switches_webcontrols_units_and_home_with_it() {
    let mut rig = Rig::connected_with(settings(&[("Advanced Settings", "homeX", json!("25.4"))]));
    let effects = {
        rig.machine.load_gcode(&file(&["G20", "G0 X1"]));
        rig.act("startRun")
    };
    assert_eq!(written(&effects), ["G20"]);
    assert_eq!(pushed(&effects, "requestedSetting")[0], json!({"setting": "units", "value": "INCHES"}));
    assert_eq!(pushed(&effects, "homePositionMessage"), [json!({"xval": 1.0, "yval": 0.0})]);
    assert_eq!(rig.run_to_quiet().last().map(String::as_str), Some("G0 X2"));
}

#[test]
fn buffered_streaming_against_the_simulator_never_overfills_its_ring() {
    let mut rig =
        SimRig::with(settings(&[("Maslow Settings", "zAxis", json!(1)), ("Maslow Settings", "bufferOn", json!(1))]));
    rig.sim.move_ms = 500;
    let lines: Vec<String> = (0..60).map(|n| format!("G1 X{n} Y{} F800", n % 7)).collect();
    rig.machine.load_gcode(&lines.join("\n"));
    rig.act("startRun");
    rig.run(40_000);
    assert!(!rig.machine.snapshot().job_in_progress);
    assert_eq!(rig.sim.overflows(), 0);
    assert!(rig.sim.max_unread() <= 127, "{}", rig.sim.max_unread());
    assert!(rig.sim.max_unread() > 60, "buffered: several lines wait in the ring");
    assert_eq!(rig.sim.position()[0], 59.0);
}

/// The checklist's air cut, simulated: a file with an M0 and a tool change, Pause and Resume, Stop, then a restart
/// mid-file that finishes the job.
#[test]
fn an_air_cut_against_the_simulator_runs_every_pause_and_ends_where_the_file_does() {
    let lines = [
        "G21",
        "G90",
        "G0 Z5",
        "G0 X10 Y10",
        "G1 Z-1 F100",
        "G1 X20 F600",
        "M0",
        "G1 Y20",
        "G0 Z5",
        "T2 M6",
        "G0 X30 Y30",
        "G1 Z-1 F100",
        "G1 X40",
        "G1 Y40",
        "G0 Z5",
        "G0 X0 Y0",
    ];
    let mut rig = SimRig::new();
    rig.sim.move_ms = 30;
    rig.machine.load_gcode(&file(&lines));
    let state = |rig: &SimRig| {
        let snapshot = rig.machine.snapshot();
        (snapshot.upload_flag, snapshot.job_in_progress, snapshot.gcode_index)
    };
    rig.act("startRun");
    rig.run(2000);
    assert_eq!(state(&rig), (0, true, 7), "held at the M0");
    assert!(rig.sim.is_paused());
    rig.act("resumeRun");
    rig.run(2000);
    assert_eq!(state(&rig), (0, true, 10), "held at the tool change");
    assert_eq!(rig.machine.snapshot().current_tool, 2);
    rig.act_with("moveZ", json!("raise"), json!("10"));
    rig.run(500);
    rig.act("resumeRun");
    rig.run(150);
    rig.act("pauseRun");
    rig.run(1000);
    assert_eq!(state(&rig).0, 2);
    rig.act("resumeRun");
    rig.run(100);
    rig.act("stopRun");
    rig.run(1000);
    assert_eq!(state(&rig), (0, false, 0));
    assert!(!rig.sim.is_paused());
    rig.act_with("moveGcodeGoto", json!(12), json!(""));
    rig.take();
    rig.act("startRun");
    rig.run(3000);
    assert_eq!(state(&rig), (0, false, 0), "the restart and the rest of the file ran");
    let (_, written) = rig.take();
    assert_eq!(written[..4], ["G21  ", "G90  ", "G0 Z5.0  ", "G0 X30.0 Y30.0  "]);
    assert!(written.contains(&"G1 Z-1.0 F100.0  ".to_string()), "{written:?}");
    assert_eq!(rig.sim.position(), [0.0, 0.0, 5.0]);
    assert_eq!(rig.sim.overflows(), 0);
}

#[test]
fn moving_the_controller_to_another_port_mid_job_pauses_it() {
    let mut rig = Rig::connected_with(settings(&[("Maslow Settings", "COMport", json!("/dev/ttyACM0"))]));
    rig.machine.load_gcode(&file(&["G0 X1", "G0 X2"]));
    rig.act("startRun");
    let body = json!({"values": {"COMport": "/dev/ttyUSB0"}});
    let request = wc_machine::Request::UpdateSettings { section: "maslow".into(), body };
    rig.send(Event::Edit { id: 1, request });
    let snapshot = rig.machine.snapshot();
    assert_eq!((snapshot.upload_flag, snapshot.job_in_progress, snapshot.gcode_index), (2, true, 0));
}

#[test]
fn an_inch_files_line_position_is_shown_in_the_machines_mm() {
    let mut rig = Rig::connected();
    let gcode = wc_machine::files::Gcode::parse("G0 X2\nG20\nG0 X1\n", &settings(&[])).unwrap();
    let load = Request::LoadGcode { gcode: Box::new(gcode), file: None, origin: Origin::Open };
    rig.send(Event::Request { id: 1, request: load });
    let effects = rig.act("stopRun");
    assert_eq!(pushed(&effects, "gcodePositionMessage")[0]["xval"], 2.0 * 25.4);
}
