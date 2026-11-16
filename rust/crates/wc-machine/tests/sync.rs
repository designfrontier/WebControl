//! B2: a boot writes only what its `$$` shows differs, plus `$13`; a `$38` write restarts the controller through its
//! port, and nothing moves until it has booted again.

mod rig;

use rig::sim::SimRig;
use rig::{Rig, alerts, settings, written};
use serde_json::json;
use wc_machine::{Effect, Event, Request};
use wc_protocol::main::LinkStatus;
use wc_sim::{Build, Sim};

/// What a 51.27 controller holding webcontrol.json's defaults prints for `$$`, with `changes` applied.
fn report(changes: &[(u32, &str)]) -> Vec<String> {
    let held = [
        (0, "2438.40"),
        (1, "1219.19995117"),
        (2, "2978.39990234"),
        (3, "463.00000000"),
        (4, "310.00000000"),
        (5, "139.00000000"),
        (6, "79.00000000"),
        (7, "2"),
        (8, "139.10000610"),
        (9, "2000"),
        (10, "3360"),
        (11, "1651"),
        (12, "8113.72998046"),
        (13, "63.50000000"),
        (15, "800"),
        (16, "0"),
        (17, "0"),
        (18, "12.60000038"),
        (19, "3.17000007"),
        (20, "7560.00000000"),
        (21, "1300.00000000"),
        (22, "0.00000000"),
        (23, "34.00000000"),
        (24, "1.00000000"),
        (25, "5.00000000"),
        (26, "0.00000000"),
        (27, "0.27999999"),
        (28, "1.00000000"),
        (29, "1300.00000000"),
        (30, "0.00000000"),
        (31, "34.00000000"),
        (32, "1.00000000"),
        (33, "5.00000000"),
        (34, "0.00000000"),
        (35, "0.27999999"),
        (36, "1.00000000"),
        (37, "33.00000000"),
        (38, "2"),
        (39, "3"),
        (40, "0.00000000"),
        (41, "0.00000000"),
        (42, "2.00000000"),
        (43, "0.00000000"),
        (44, "0.00000000"),
        (45, "0.00000517"),
        (46, "97.90000152"),
    ];
    held.iter()
        .map(|&(key, value)| {
            let value = changes.iter().find(|(k, _)| *k == key).map_or(value, |(_, v)| v);
            format!("${key}={value}")
        })
        .chain([String::new()])
        .collect()
}

const HANDSHAKE: [&str; 4] = ["B05", "G21", "G90", "$$"];

fn handshake_then(lines: &[&str]) -> Vec<String> {
    HANDSHAKE.iter().chain(lines).map(|l| l.to_string()).collect()
}

fn restarted(rig: &Rig) -> bool {
    rig.machine.snapshot().link == LinkStatus::Disconnected
}

#[test]
fn a_controller_holding_every_setting_gets_only_r() {
    let mut rig = Rig::new();
    assert_eq!(rig.boot_reporting("51.27", &report(&[])), handshake_then(&["$13=63.5"]));
    assert!(!restarted(&rig));
}

#[test]
fn one_differing_setting_is_written_with_r_then_read_back() {
    let mut rig = Rig::new();
    let sent = rig.boot_reporting("51.27", &report(&[(3, "478.00000000")]));
    // This controller never takes the write, so the read-back rewrites it once more and reads nothing again.
    assert_eq!(sent, handshake_then(&["$3=463", "$13=63.5", "$$", "$3=463"]));
    assert!(!restarted(&rig));
}

/// The owner's machine on 2026-11-15: every connect wrote `$38`, and no motor moved afterwards.
#[test]
fn a_differing_chain_over_sprocket_restarts_the_controller_before_anything_moves() {
    let mut rig = Rig::new();
    let mut effects = Vec::new();
    rig.open();
    ["PCB v1.6 TLE9201  Detected", "Grbl v1.00", "ready", "ok"].iter().for_each(|l| effects.extend(rig.rx(l)));
    let sent = rig.answer_with("51.27", &report(&[(38, "1")]));
    assert_eq!(sent, handshake_then(&["$13=63.5", "$38=2"]), "no read-back: the restart replaces it");
    assert!(restarted(&rig));
    let jog = rig.act_with("move", json!("up"), json!("1"));
    assert!(written(&jog).is_empty());
    assert!(alerts(&jog)[0].contains("restarting the controller"), "{:?}", alerts(&jog));
    let sent = rig.boot_reporting("51.27", &report(&[]));
    assert_eq!(sent, handshake_then(&["$13=63.5"]), "$38 now matches: no second write, no second restart");
    assert!(!restarted(&rig));
    rig.act_with("move", json!("up"), json!("1"));
    assert!(rig.run_to_quiet().contains(&"G91".to_string()), "moves go once it has booted");
}

#[test]
fn the_restart_closes_and_reopens_the_same_port() {
    let mut rig = Rig::new();
    rig.open();
    ["PCB v1.6 TLE9201  Detected", "Grbl v1.00", "ready", "ok"].iter().for_each(|l| {
        rig.rx(l);
    });
    rig.answer_until_quiet("51.27");
    rig.rx("<Idle,MPos:0.00,0.00,0.00,WPos:0.000,0.000,0.000>");
    rig.send(Event::Edit {
        id: 1,
        request: Request::UpdateSettings {
            section: "advanced".into(),
            body: json!({"values": {"chainOverSprocket": "Top"}}),
        },
    });
    let mut effects = rig.advance(100);
    effects.extend(rig.rx("ok"));
    effects.extend(rig.advance(100));
    let close = effects.iter().position(|e| *e == Effect::ClosePort).expect("closed");
    let open = effects.iter().position(|e| *e == Effect::OpenPort("/dev/ttyACM0".into())).expect("reopened");
    assert!(close < open);
}

#[test]
fn a_ui_change_of_chain_over_sprocket_is_written_then_restarts() {
    let mut rig = Rig::connected();
    rig.clear_written();
    let body = json!({"values": {"chainOverSprocket": "Top"}});
    rig.send(Event::Edit { id: 1, request: Request::UpdateSettings { section: "advanced".into(), body } });
    assert_eq!(rig.run_to_quiet(), ["$38=1"]);
    assert!(restarted(&rig));
    let sent = rig.boot_reporting("51.27", &report(&[(38, "1")]));
    assert_eq!(sent, handshake_then(&["$13=63.5"]));
}

#[test]
fn a_board_that_does_not_reset_is_reported_and_moves_wait_for_a_banner() {
    let mut rig = Rig::new();
    rig.boot_reporting("51.27", &report(&[(38, "1")]));
    assert!(restarted(&rig));
    rig.open();
    let effects = rig.advance(3100);
    assert!(alerts(&effects).iter().any(|a| a.contains("did not restart")), "{:?}", alerts(&effects));
    let sent = rig.answer_with("51.27", &report(&[]));
    assert_eq!(sent, handshake_then(&["$13=63.5"]));
    let jog = rig.act_with("move", json!("up"), json!("1"));
    assert!(alerts(&jog)[0].contains("did not restart"));
    assert!(rig.run_to_quiet().is_empty());
    rig.boot_reporting("51.27", &report(&[]));
    rig.act_with("move", json!("up"), json!("1"));
    assert!(rig.run_to_quiet().contains(&"G91".to_string()));
}

#[test]
fn a_value_that_does_not_stick_is_not_written_again_after_the_restart() {
    let mut rig = Rig::new();
    rig.boot_reporting("51.27", &report(&[(38, "1")]));
    rig.open();
    ["PCB v1.6 TLE9201  Detected", "Grbl v1.00", "ready", "ok"].iter().for_each(|l| {
        rig.rx(l);
    });
    let sent = rig.answer_with("51.27", &report(&[(38, "1")]));
    assert_eq!(sent, handshake_then(&["$13=63.5"]), "no loop");
    assert!(!restarted(&rig));
    let next = rig.boot_reporting("51.27", &report(&[(38, "1")]));
    assert_eq!(next, handshake_then(&["$13=63.5", "$38=2"]), "a later boot tries once more");
}

#[test]
fn a_read_back_after_a_restart_that_did_not_stick_never_writes_chain_over_sprocket_again() {
    let mut rig = Rig::new();
    rig.boot_reporting("51.27", &report(&[(38, "1")]));
    let sent = rig.boot_reporting("51.27", &report(&[(38, "1"), (3, "478.00000000")]));
    assert_eq!(sent, handshake_then(&["$3=463", "$13=63.5", "$$", "$3=463"]), "no $38 from the read-back either");
    assert!(!restarted(&rig));
}

#[test]
fn an_eeprom_reset_skips_a_chain_over_sprocket_the_controller_already_holds() {
    let mut rig = Rig::new();
    rig.open();
    let banner = ["PCB v1.6 TLE9201  Detected", "error: EEPROM read fail. Using default settings.", "Grbl v1.00"];
    banner.iter().chain(&["ready", "ok"]).for_each(|l| {
        rig.rx(l);
    });
    let sent = rig.answer_with("51.27", &report(&[]));
    assert!(sent.contains(&"$37=33".to_string()), "the full push: {sent:?}");
    assert!(!sent.iter().any(|l| l.starts_with("$38=")), "{sent:?}");
    assert_eq!(sent.last().map(String::as_str), Some("$$"), "read back, no restart");
    assert!(!restarted(&rig));
}

#[test]
fn a_paused_job_never_gets_a_chain_over_sprocket_write() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode("G0 X1\nG0 X2\nG0 X3\n");
    rig.act("startRun");
    rig.advance(60);
    rig.rx("ok");
    rig.act("pauseRun");
    rig.run_to_quiet();
    rig.act("reportSettings");
    rig.advance(100);
    let mut effects = Vec::new();
    report(&[(38, "1")]).iter().for_each(|line| effects.extend(rig.rx(line)));
    effects.extend(rig.rx("ok"));
    effects.extend(rig.advance(200));
    assert!(!written(&effects).iter().any(|l| l.starts_with("$38=")), "{:?}", written(&effects));
    assert!(alerts(&effects).iter().any(|a| a.contains("Stop the job first")), "{:?}", alerts(&effects));
    assert!(!restarted(&rig));
}

#[test]
fn a_chain_over_sprocket_change_during_an_alarm_hold_still_restarts_and_the_hold_stays() {
    let mut rig = Rig::connected();
    rig.rx("ALARM: The sled is not keeping up with its expected position and has halted.");
    rig.advance(300);
    rig.rx("<Idle,MPos:0.00,0.00,0.00,WPos:0.000,0.000,0.000>");
    rig.clear_written();
    let body = json!({"values": {"chainOverSprocket": "Top"}});
    rig.send(Event::Edit { id: 1, request: Request::UpdateSettings { section: "advanced".into(), body } });
    assert_eq!(rig.run_to_quiet(), ["$38=1"]);
    assert!(restarted(&rig));
    rig.reboot();
    let jog = rig.act_with("move", json!("up"), json!("1"));
    assert!(alerts(&jog)[0].contains("alarm"), "the alarm still holds until Stop: {:?}", alerts(&jog));
}

/// Against the simulator, which stops following moves after a `$38` write as the owner's board did.
#[test]
fn a_simulated_connect_with_a_differing_chain_over_sprocket_still_jogs() {
    let sim = Sim::new(Build::holey_51_27()).with_setting(38, 1.0);
    let mut rig = SimRig::on(settings(&[("Maslow Settings", "zAxis", json!(1))]), sim);
    rig.run(3000);
    assert_eq!(rig.sim.setting(38), Some(2.0));
    assert!(!rig.sim.axes_reinitialised(), "rebooted since the write");
    rig.take();
    rig.act_with("move", json!("up"), json!("10"));
    rig.run(500);
    let (effects, _) = rig.take();
    assert!(alerts(&effects).is_empty(), "{:?}", alerts(&effects));
    assert!(rig.sim.position()[1] > 0.0, "the sled moved: {:?}", rig.sim.position());
}

#[test]
fn a_chain_over_sprocket_line_in_a_job_file_is_skipped_and_the_job_goes_on() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode("G0 X1\n$38=1\nG0 X2\n");
    rig.clear_written();
    rig.act("startRun");
    let mut effects = rig.advance(60);
    (0..4).for_each(|_| {
        effects.extend(rig.rx("ok"));
        effects.extend(rig.advance(60));
    });
    let sent = written(&effects);
    assert!(sent.contains(&"G0 X2".to_string()) && !sent.iter().any(|l| l.starts_with("$38")), "{sent:?}");
    assert!(
        alerts(&effects).iter().any(|a| a.starts_with("Line 2 sets chain-over-sprocket")),
        "{:?}",
        alerts(&effects)
    );
    assert!(!restarted(&rig));
}

#[test]
fn typed_gcode_waits_for_the_board_that_did_not_restart() {
    let mut rig = Rig::new();
    rig.boot_reporting("51.27", &report(&[(38, "1")]));
    rig.open();
    rig.advance(3100);
    rig.answer_with("51.27", &report(&[]));
    let effects = rig.send(Event::Request { id: 1, request: wc_machine::files::Request::SendGcode("G0 X10".into()) });
    assert!(alerts(&effects).iter().any(|a| a.contains("did not restart")), "{:?}", alerts(&effects));
    assert!(rig.run_to_quiet().is_empty());
}
