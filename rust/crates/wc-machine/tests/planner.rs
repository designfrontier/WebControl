//! A planner build's line protocol beyond the transcripts: Pause and its `G4 P0`, an alarm resuming at the reported
//! line, the lost-ack guards, and an `rs` for a line WebControl no longer holds.

mod rig;

use rig::{Rig, alerts, written};
use wc_machine::Event;

const CAPABILITIES: &str = "Capabilities: planner line-numbers accel-settings status-line";
const STILL: &str = "<Idle,MPos:0.00,0.00,0.00,WPos:0.000,0.000,0.000>";

fn number(line: &str) -> Option<&str> {
    line.strip_prefix('N')?.split_once(' ').map(|(number, _)| number)
}

/// Answers every line as a planner build would, until the machine writes nothing more.
fn answer(rig: &mut Rig, mut lines: Vec<String>) -> Vec<String> {
    let mut all = Vec::new();
    while !lines.is_empty() {
        let replies: Vec<String> = lines
            .iter()
            .flat_map(|line| match (line.trim(), number(line)) {
                ("B05", _) => vec!["Firmware Version 51.29".into(), CAPABILITIES.into(), "ok".into()],
                (_, Some(number)) => vec![format!("ok N{number}")],
                _ => vec!["ok".into()],
            })
            .collect();
        all.append(&mut lines);
        lines = replies.iter().flat_map(|reply| written(&rig.rx(reply))).collect();
        lines.extend(written(&rig.advance(100)));
    }
    all
}

fn planner() -> Rig {
    let mut rig = Rig::new();
    rig.open();
    let boot: Vec<String> =
        ["PCB v1.1 Detected", "Grbl v1.00", "ready", "ok"].iter().flat_map(|line| written(&rig.rx(line))).collect();
    answer(&mut rig, boot);
    rig.rx(STILL);
    rig.advance(100);
    rig
}

fn report(line: u64, queued: u32, space: u32) -> String {
    format!("[PE:0.00,0.00,{space},1650.50,1651.25,{line},{queued}]")
}

/// Job lines go one per pass, so a few ticks send all that fit.
fn start(rig: &mut Rig, lines: &[&str]) -> Vec<String> {
    rig.machine.load_gcode(&lines.iter().map(|line| format!("{line}\n")).collect::<String>());
    let mut sent = written(&rig.act("startRun"));
    sent.extend(written(&rig.advance(100)));
    sent
}

#[test]
fn the_handshake_after_the_capability_is_numbered() {
    let mut rig = Rig::new();
    rig.open();
    let boot: Vec<String> =
        ["PCB v1.1 Detected", "Grbl v1.00", "ready", "ok"].iter().flat_map(|line| written(&rig.rx(line))).collect();
    let sent = answer(&mut rig, boot);
    assert_eq!(&sent[..4], ["B05", "M110 N1", "N1 G21*27", "N2 G90*18"]);
    assert!(rig.machine.snapshot().planner);
}

#[test]
fn pause_waits_for_g4_p0() {
    let mut rig = planner();
    rig.machine.load_gcode("G1 X1 F600\nG1 X2\nG1 X3\n");
    assert_eq!(written(&rig.act("startRun")), ["M110 N5", "N5 G1 X1 F600*52"]);
    rig.act("pauseRun");
    assert!(written(&rig.rx("ok")).is_empty(), "M110's ok frees only itself");
    assert_eq!(written(&rig.rx("ok N5")), ["N6 G4 P0*107"], "queued moves run out; the dwell says when");
    rig.rx(STILL);
    rig.advance(1100);
    assert!(written(&rig.rx(&report(5, 1, 127))).is_empty(), "the dwell waits for the move: no lost ack");
    rig.rx("ok N6");
    assert_eq!(rig.machine.snapshot().upload_flag, 2);
    assert_eq!(rig.machine.snapshot().gcode_index, 1);
}

#[test]
fn an_alarm_resumes_at_the_line_the_report_named() {
    let mut rig = planner();
    start(&mut rig, &["G1 X1 F600", "G1 X2", "G1 X3"]);
    ["ok", "ok N5", "ok N6", "ok N7"].iter().for_each(|line| {
        rig.rx(line);
    });
    rig.rx(&report(6, 2, 127));
    rig.rx("ALARM: The sled is not keeping up with its expected position and has halted.");
    assert_eq!(rig.machine.snapshot().gcode_index, 1, "N6 is file line 2");
}

#[test]
fn a_firmware_pause_is_no_lost_ack() {
    let mut rig = planner();
    start(&mut rig, &["G1 X1 F600", "M0", "G1 X3"]);
    ["ok", "ok N5"].iter().for_each(|line| {
        rig.rx(line);
    });
    rig.rx("Maslow Paused");
    rig.advance(1100);
    assert!(written(&rig.rx(&report(6, 0, 127))).is_empty());
}

#[test]
fn an_rs_for_a_line_no_longer_held_renumbers() {
    let mut rig = planner();
    start(&mut rig, &["G1 X1 F600", "G1 X2"]);
    rig.rx("ok");
    rig.rx("ok N5");
    assert_eq!(written(&rig.rx("ok N6")), ["N7 G4 P0*106"]);
    assert_eq!(written(&rig.rx("rs N3")), ["M110 N7", "N7 G4 P0*106"]);
    rig.rx("ok");
    rig.rx("ok N7");
    assert_eq!(rig.machine.snapshot().upload_flag, 0);
}

#[test]
fn an_error_for_the_resent_line_frees_every_copy() {
    let mut rig = planner();
    start(&mut rig, &["G1 X1 F600", "G1 X2", "G1 X3"]);
    rig.rx("ok");
    rig.rx("ok N5");
    assert_eq!(written(&rig.rx("rs N6")), ["N6 G1 X2*100"]);
    assert_eq!(written(&rig.rx("error: Invalid statement")), ["N7 G1 X3*100"]);
}

#[test]
fn a_lost_ack_sends_every_line_not_acked_again() {
    let mut rig = planner();
    let long = "G1 X100.123456 Y200.123456 Z-1.234567 F1000";
    let sent = start(&mut rig, &[long, long, long, long]);
    assert_eq!(sent.len(), 3, "M110 and two lines fill the ring");
    rig.rx("ok");
    rig.advance(1100);
    let again = written(&rig.rx(&report(0, 0, 127)));
    assert_eq!(again, sent[1..]);
    let effects = rig.send(Event::Tick);
    assert!(alerts(&effects).is_empty());
}

#[test]
fn a_build_without_the_capability_is_not_numbered() {
    let mut rig = planner();
    rig.rx("Firmware Version 51.29");
    rig.rx("ok");
    assert!(!rig.machine.snapshot().planner);
    rig.rx("rs N3");
    assert_eq!(written(&rig.act_with("move", "up".into(), "1".into())), ["G91"]);
}
