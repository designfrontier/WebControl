//! The alarm hold: after `ALARM:`, `Buffer overflow!` or an unplaceable sled nothing that moves the machine is
//! written until Stop, or Play on the job the alarm held.

mod rig;

use rig::sim::SimRig;
use rig::{Rig, alerts, commands, pushed, settings, written};
use serde_json::{Value, json};
use wc_machine::calibration::Kind;
use wc_machine::files::{Reply, Request as FileRequest};
use wc_machine::{Effect, Event, Request};

const SLED_NOT_KEEPING_UP: &str = "ALARM: The sled is not keeping up with its expected position and has halted. \
                                   Click the 'Stop' button to clear the alarm. More information at: \
                                   https://github.com/MaslowCNC/Firmware/wiki/Keeping-Up  ";
const HELD: &str = "Press Stop to clear it before sending it anything else.";
const JOB_HELD: &str = "Press Stop to clear it, or Play to resume the job";
const IDLE: &str = "<Idle,MPos:41.67,336.94,11.05,WPos:0.000,0.000,0.000>";
const PE: &str = "[PE:0.00,0.00,127,1589.90,1513.52]";
const JOB: [&str; 8] = ["G21", "G90", "G0 X1", "G0 X2", "G0 X3", "G0 X4", "G0 X5", "G0 X6"];

fn file(lines: &[&str]) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
}

fn owners_home() -> Rig {
    Rig::connected_with(settings(&[
        ("Advanced Settings", "homeX", json!("41.6763")),
        ("Advanced Settings", "homeY", json!("60.1447")),
    ]))
}

/// The firmware's status report and `[PE:]` line every 200 ms for `ms`, with everything written meanwhile.
fn reports_for(rig: &mut Rig, ms: u64) -> Vec<String> {
    (0..ms / 200)
        .flat_map(|_| {
            let mut effects = rig.advance(200);
            effects.extend(rig.rx(IDLE));
            effects.extend(rig.rx(PE));
            written(&effects)
        })
        .collect()
}

/// The `!`, its `ok`, the position reload and the report that ends the resync.
fn stop(rig: &mut Rig) -> Vec<Effect> {
    let mut effects = rig.act("stopRun");
    ["ok", "[Forward Calculating Position]", "position loaded at:", "41.67", "336.94"].iter().for_each(|line| {
        effects.extend(rig.rx(line));
    });
    effects.extend(rig.advance(300));
    effects.extend(rig.rx(IDLE));
    effects
}

fn alarm_mid_home() -> Rig {
    let mut rig = owners_home();
    rig.rx(IDLE);
    rig.clear_written();
    let mut sent = written(&rig.act("home"));
    [("G90", 60), ("G00 Z11.06", 60)].iter().for_each(|(echo, wait)| {
        sent.extend(written(&rig.rx(echo)));
        sent.extend(written(&rig.rx("ok")));
        sent.extend(written(&rig.advance(*wait)));
    });
    assert_eq!(sent, ["G90", "G00 Z11.06", "G00 X41.6763 Y60.1447"]);
    rig.rx("G00 X41.6763 Y60.1447");
    rig.rx("<Idle,MPos:41.67,334.14,11.05,WPos:0.000,0.000,0.000>");
    rig.rx("[PE:-1.24,-1.29,127,1589.90,1513.52]");
    let alarm = rig.rx(SLED_NOT_KEEPING_UP);
    assert_eq!(pushed(&alarm, "alarm"), [json!({"message": "Alarm: Sled Not Keeping Up"})]);
    rig
}

#[test]
fn an_alarm_mid_home_never_sends_the_plunge() {
    let mut rig = alarm_mid_home();
    let mut after = Vec::new();
    ["[Forward Calculating Position]", "position loaded at:", "41.67", "336.94"].iter().for_each(|line| {
        after.extend(written(&rig.rx(line)));
    });
    after.extend(reports_for(&mut rig, 3000));
    assert!(after.is_empty(), "nothing after the alarm, G00 Z0 least of all: {after:?}");
    let refused = rig.act_with("move", json!("up"), json!("10"));
    assert!(alerts(&refused)[0].ends_with(HELD), "{refused:?}");
    assert!(written(&refused).is_empty() && reports_for(&mut rig, 1000).is_empty());
    let stopped = stop(&mut rig);
    assert_eq!(written(&stopped), ["!"]);
    assert!(commands(&stopped).contains(&"clearAlarm".to_string()));
    rig.clear_written();
    rig.act_with("move", json!("up"), json!("10"));
    assert_eq!(rig.run_to_quiet(), ["G91", "G00 Y10.0", "G90"], "Stop cleared the alarm");
}

#[test]
fn the_simulated_controller_keeps_the_bit_where_the_alarm_left_it() {
    let mut rig = SimRig::new();
    rig.run(3000);
    rig.take();
    rig.act("home");
    while !rig.written.iter().any(|line| line.starts_with("G00 Z5.0")) {
        rig.run(10);
    }
    rig.sim.move_ms = 60_000;
    rig.run(500);
    let travel = rig.take().1;
    assert_eq!(travel.last().map(|line| line.trim_end()), Some("G00 X0.0 Y0.0"), "{travel:?}");
    let z = rig.sim.position()[2];
    let lines = rig.sim.alarm("The sled is not keeping up with its expected position and has halted.");
    rig.deliver(lines);
    rig.run(3000);
    let after = rig.take().1;
    assert!(after.is_empty(), "{after:?}");
    assert_eq!(rig.sim.position()[2], z);
}

fn job_halted(buffered: bool) -> (Rig, usize) {
    let mut rig = Rig::connected_with(settings(&[("Maslow Settings", "bufferOn", json!(i32::from(buffered)))]));
    rig.machine.load_gcode(&file(&JOB));
    rig.act("startRun");
    (0..3).for_each(|_| {
        rig.advance(60);
        rig.rx("ok");
    });
    rig.advance(60);
    let unacked = 3;
    rig.rx(SLED_NOT_KEEPING_UP);
    (rig, unacked)
}

#[test]
fn an_alarm_mid_job_sends_no_more_lines_and_holds_the_job_at_the_dropped_line() {
    [false, true].into_iter().for_each(|buffered| {
        let (mut rig, line) = job_halted(buffered);
        assert!(reports_for(&mut rig, 3000).is_empty(), "buffered {buffered}");
        let snapshot = rig.machine.snapshot();
        assert_eq!((snapshot.job_in_progress, snapshot.upload_flag, snapshot.gcode_index), (true, 0, line));
        let jog = rig.act_with("move", json!("up"), json!("10"));
        assert!(alerts(&jog)[0].contains(JOB_HELD), "{jog:?}");
        assert!(reports_for(&mut rig, 400).is_empty());
        rig.clear_written();
        let play = rig.act("startRun");
        assert_eq!(written(&play), ["~"]);
        assert!(commands(&play).contains(&"clearAlarm".to_string()), "Play on the held job clears the alarm");
        rig.rx("ok");
        let resumed = rig.run_to_quiet();
        assert!(resumed.contains(&"G0 X3".to_string()), "{resumed:?}");
        assert!(alerts(&rig.act_with("move", json!("up"), json!("10"))).iter().all(|a| !a.contains("alarm")));
    });
}

#[test]
fn stop_after_an_alarm_mid_job_ends_the_job_and_clears_it() {
    let (mut rig, _) = job_halted(false);
    stop(&mut rig);
    let snapshot = rig.machine.snapshot();
    assert_eq!((snapshot.job_in_progress, snapshot.gcode_index), (false, 0));
    rig.clear_written();
    rig.act("defineZ0");
    assert_eq!(rig.run_to_quiet(), ["G10 Z0"]);
}

#[test]
fn a_jog_queued_behind_a_move_never_moves_after_an_alarm() {
    let mut rig = Rig::connected();
    rig.clear_written();
    assert_eq!(written(&rig.act_with("move", json!("right"), json!("10"))), ["G91"]);
    rig.rx("ok");
    assert_eq!(written(&rig.advance(60)), ["G00 X10.0"]);
    rig.act_with("move", json!("up"), json!("10"));
    rig.rx(SLED_NOT_KEEPING_UP);
    let after = reports_for(&mut rig, 3000);
    assert!(after.iter().all(|line| !line.contains(['X', 'Y', 'Z'])), "{after:?}");
    assert_eq!(after.last().map(String::as_str), Some("G90"), "the jogs' G90 leaves the firmware absolute");
    assert_eq!(rig.machine.snapshot().positioning, wc_gcode::Positioning::Absolute);
}

#[test]
fn a_macro_queued_behind_a_move_is_dropped_and_refused_until_stop() {
    let mut rig = Rig::connected_with(settings(&[("Maslow Settings", "macro1", json!("G0 X100 Y100"))]));
    rig.clear_written();
    rig.act("macro2");
    rig.act("macro1");
    rig.rx(SLED_NOT_KEEPING_UP);
    assert!(reports_for(&mut rig, 3000).iter().all(|line| !line.contains("X100")));
    assert!(alerts(&rig.act("macro1"))[0].ends_with(HELD));
    stop(&mut rig);
    rig.clear_written();
    rig.act("macro1");
    assert_eq!(rig.run_to_quiet(), ["G0 X100 Y100"]);
}

#[test]
fn a_return_to_center_waiting_for_the_bits_height_is_dropped() {
    let mut rig = Rig::connected();
    rig.act("stopRun");
    rig.rx("ok");
    rig.advance(300);
    rig.rx(IDLE);
    rig.clear_written();
    rig.act("returnToCenter");
    rig.rx(SLED_NOT_KEEPING_UP);
    assert!(reports_for(&mut rig, 4000).iter().all(|line| !line.contains("X0.0")));
}

const MOVES: [&str; 25] = [
    "move",
    "moveTo",
    "moveZ",
    "home",
    "returnToCenter",
    "moveToDefault",
    "defineZ0",
    "touchZ",
    "macro1",
    "macro2",
    "setMaxZ",
    "setMinZ",
    "clearZ",
    "getZlimits",
    "startRun",
    "testMotors",
    "setSprocketsZero",
    "rotateSprocket",
    "resetChainLengths",
    "wipeEEPROM",
    "cutTriangularCalibrationPattern",
    "acceptHoleyCalibrationResults",
    "executeVelocityPIDTest",
    "upgradeHoleyFirmware",
    "noSuchCommand",
];

#[test]
fn every_action_that_reaches_the_controller_is_refused_and_the_rest_are_not() {
    let mut rig = alarm_mid_home();
    rig.machine.load_gcode(&file(&JOB));
    MOVES.iter().for_each(|command| {
        let effects = rig.act_with(command, json!("up"), json!("1"));
        assert_eq!(alerts(&effects), [format!("The controller raised an alarm and halted. {HELD}")], "{command}");
        assert!(written(&effects).is_empty(), "{command}");
    });
    ["statusRequest", "clearLogs", "pauseRun", "moveGcodeGoto", "boardClearBoard"].iter().for_each(|command| {
        assert!(alerts(&rig.act_with(command, json!(1), json!(""))).is_empty(), "{command}");
    });
    assert!(alerts(&rig.act_with("defineHome", json!(1), json!(2))).is_empty());
    assert_eq!(written(&rig.act("resumeRun")), ["~"], "with no job, Resume only sends ~");
    rig.advance(300);
    rig.rx(IDLE);
    assert_eq!(written(&rig.act("reportSettings")), ["$$"], "$ lines never move the machine");
}

fn respond(rig: &mut Rig, request: Request) -> (u16, Value) {
    rig.send(Event::Edit { id: 1, request })
        .into_iter()
        .find_map(|effect| match effect {
            Effect::Respond { response, .. } => Some((response.status, response.body)),
            _ => None,
        })
        .unwrap()
}

#[test]
fn http_writes_that_move_are_refused_and_settings_still_reach_the_controller() {
    let mut rig = alarm_mid_home();
    rig.advance(300);
    rig.rx(IDLE);
    let sent = rig.send(Event::Request { id: 1, request: FileRequest::SendGcode("G0 X5".into()) });
    assert!(sent.iter().any(|e| matches!(e, Effect::Reply { reply: Reply::Failed, .. })));
    assert!(alerts(&sent)[0].ends_with(HELD) && written(&sent).is_empty());
    let pid = json!({"kind": "velocity", "motor": "L", "start": 0, "stop": 1, "steps": 1, "version": 1, "kp": 1,
        "ki": 1, "kd": 1});
    assert_eq!(respond(&mut rig, Request::PidTest(pid)).0, 409);
    let (status, body) = respond(&mut rig, Request::AcceptCalibration(Kind::Triangular));
    assert_eq!((status, body["error"].as_str().is_some_and(|e| e.ends_with(HELD))), (409, true));
    let body = json!({"values": {"motorSpacingX": "3000"}});
    rig.clear_written();
    let saved = respond(&mut rig, Request::UpdateSettings { section: "maslow".into(), body });
    assert_eq!(saved.0, 200, "{saved:?}");
    let settings = rig.run_to_quiet();
    assert!(!settings.is_empty() && settings.iter().all(|line| line.starts_with('$')), "{settings:?}");
}

#[test]
fn an_overflow_holds_too() {
    let mut rig = Rig::connected();
    rig.clear_written();
    rig.act_with("move", json!("right"), json!("10"));
    rig.act("home");
    let overflow = rig.rx("Buffer overflow!");
    assert_eq!(pushed(&overflow, "alarm"), [json!({"message": "Alarm: Controller Buffer Overflow"})]);
    assert!(reports_for(&mut rig, 3000).iter().all(|line| !line.contains(['X', 'Y', 'Z'])));
    assert!(alerts(&rig.act("home"))[0].ends_with(HELD));
}

#[test]
fn an_unplaceable_sled_holds_until_stop_and_stops_own_reload_does_not_hold_again() {
    let mut rig = Rig::connected();
    let unable = "Message: Unable to find valid machine position for chain lengths 1.00, 2.00";
    assert_eq!(pushed(&rig.rx(unable), "alarm"), [json!({"message": unable})]);
    assert!(alerts(&rig.act("home"))[0].ends_with(HELD));
    rig.act("stopRun");
    rig.rx("ok");
    rig.rx("[Forward Calculating Position]");
    assert_eq!(commands(&rig.rx(unable)), ["alarm"], "shown, not held");
    rig.advance(300);
    rig.rx(IDLE);
    rig.clear_written();
    rig.act_with("move", json!("up"), json!("1"));
    assert_eq!(rig.run_to_quiet(), ["G91", "G00 Y1.0", "G90"]);
    rig.rx(unable);
    assert!(alerts(&rig.act("home"))[0].ends_with(HELD), "after the resync it holds again");
}

#[test]
fn a_reboot_keeps_the_hold_and_shows_the_alarm_again() {
    let mut rig = alarm_mid_home();
    rig.clear_written();
    ["PCB v1.1 Detected", "Grbl v1.00", "ready", "ok"].iter().for_each(|line| {
        rig.rx(line);
    });
    let handshake = rig.run_to_quiet();
    assert_eq!(handshake.first().map(String::as_str), Some("B05"), "the handshake still goes out");
    let version = rig.rx("Firmware Version 51.27");
    assert_eq!(pushed(&version, "alarm"), [json!({"message": "Alarm: Sled Not Keeping Up"})]);
    assert!(pushed(&version, "clearAlarm").is_empty());
    assert!(alerts(&rig.act("home"))[0].ends_with(HELD));
    stop(&mut rig);
    assert!(alerts(&rig.act("home")).is_empty());
}

#[test]
fn stop_z_clears_the_alarm_too() {
    let mut rig = alarm_mid_home();
    let stopped = rig.act("stopZ");
    assert_eq!((written(&stopped), commands(&stopped).contains(&"clearAlarm".to_string())), (vec!["!".into()], true));
    assert!(alerts(&rig.act("home")).is_empty());
}

const UNABLE: &str = "Message: Unable to find valid machine position for chain lengths 1.00, 2.00";

#[test]
fn stops_reload_never_holds_again_however_long_the_firmware_takes_to_give_up() {
    let mut rig = Rig::connected();
    rig.rx(UNABLE);
    rig.act("stopRun");
    rig.rx("ok");
    rig.rx("[Forward Calculating Position]");
    reports_for(&mut rig, 1000);
    assert_eq!(commands(&rig.rx(UNABLE)), ["alarm"], "200 failed guesses outlast the resync");
    rig.clear_written();
    assert!(alerts(&rig.act_with("move", json!("up"), json!("1"))).is_empty());
    assert_eq!(rig.run_to_quiet(), ["G91", "G00 Y1.0", "G90"]);
}

#[test]
fn a_reload_that_cannot_place_the_sled_after_an_alarm_leaves_only_stop() {
    let (mut rig, _) = job_halted(false);
    rig.rx("[Forward Calculating Position]");
    rig.rx(UNABLE);
    reports_for(&mut rig, 400);
    ["startRun", "resumeRun"].iter().for_each(|command| {
        let refused = rig.act(command);
        assert!(alerts(&refused)[0].ends_with(HELD), "{command}: {refused:?}");
        assert!(written(&refused).is_empty(), "{command}");
    });
    stop(&mut rig);
    assert!(alerts(&rig.act("home")).is_empty());
}
