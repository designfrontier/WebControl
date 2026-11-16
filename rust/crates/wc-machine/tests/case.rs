//! Lower-case G-code, typed or in a file: holey 51.27 upper-cases every line before it parses it
//! (`maslowRingBuffer::prettyReadLine`), so WebControl reads each line the same way.

mod rig;

use rig::{Rig, alerts, settings, written};
use serde_json::json;
use wc_machine::Event;
use wc_machine::files::Request;
use wc_protocol::main::Units;

fn file(lines: &[&str]) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
}

fn typed(rig: &mut Rig, text: &str) -> Vec<String> {
    rig.clear_written();
    rig.send(Event::Request { id: 1, request: Request::SendGcode(text.into()) });
    rig.run_to_quiet()
}

fn stream(rig: &mut Rig, lines: &[&str]) -> Vec<String> {
    rig.machine.load_gcode(&file(lines));
    rig.clear_written();
    rig.act("startRun");
    rig.run_to_quiet()
}

#[test]
fn a_typed_g20_switches_webcontrol_to_inches_with_the_controller() {
    let mut rig = Rig::connected_with(settings(&[("Advanced Settings", "homeX", json!("254"))]));
    assert_eq!(typed(&mut rig, "g20"), ["G20"]);
    let snapshot = rig.machine.snapshot();
    assert_eq!(snapshot.units(), Units::Inches);
    assert_eq!(snapshot.settings.number("Advanced Settings", "homeX"), 10.0);
    rig.clear_written();
    rig.act_with("moveZ", json!("raise"), json!("1"));
    assert_eq!(rig.run_to_quiet(), ["G20", "G91", "G00 Z0.0394", "G90"], "the mm Z panel moves in inches");
    assert_eq!(typed(&mut rig, "g21"), ["G21"]);
    assert_eq!(rig.machine.snapshot().units(), Units::Mm);
}

#[test]
fn a_lower_case_g91_stops_the_home_shift_and_g90_resumes_it() {
    let mut rig = Rig::connected_with(settings(&[("Advanced Settings", "homeX", json!("10"))]));
    let sent = stream(&mut rig, &["g90", "g0 x1", "g91", "g0 x2", "g90", "g0 x3"]);
    assert_eq!(sent, ["G90", "G0 X11", "G91", "G0 X2", "G90", "G0 X13"]);
}

#[test]
fn a_lower_case_m0_pauses_the_job_until_resume() {
    let mut rig = Rig::connected();
    assert_eq!(stream(&mut rig, &["g0 x1", "m0", "g0 x2"]), ["G0 X1", "M0"]);
    rig.rx("Maslow Paused");
    let snapshot = rig.machine.snapshot();
    assert_eq!((snapshot.upload_flag, snapshot.gcode_index), (0, 2));
    assert_eq!(written(&rig.act("resumeRun"))[0], "~");
}

#[test]
fn a_lower_case_m6_is_a_tool_change() {
    let mut rig = Rig::connected();
    assert_eq!(stream(&mut rig, &["g1 x1", "t2", "m6", "g1 x2"]), ["G1 X1", "T2 M6"]);
    let snapshot = rig.machine.snapshot();
    assert_eq!((snapshot.upload_flag, snapshot.current_tool, snapshot.gcode_index), (-1, 2, 3));
}

#[test]
fn a_start_mid_file_restores_a_lower_case_spindle_and_units() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode(&file(&["g20", "g90", "m3 s1000", "g0 x1 y1", "g1 z-0.1 f10", "g1 x2"]));
    rig.act_with("moveGcodeGoto", json!(5), json!(""));
    rig.rx("<Idle,MPos:0.00,0.00,0.00,WPos:0.000,0.000,0.000>");
    rig.clear_written();
    rig.act("startRun");
    let sent = rig.run_to_quiet();
    assert_eq!(sent[..3], ["G20", "G90", "G0 Z0.1969"]);
    assert!(sent.contains(&"M3".to_string()), "the spindle is on before the plunge: {sent:?}");
    assert!(sent.contains(&"G1 Z-0.1 F10.0".to_string()), "{sent:?}");
    assert_eq!(rig.machine.snapshot().units(), Units::Inches);
}

#[test]
fn a_typed_lower_case_probe_holds_moves_until_it_answers() {
    let mut rig = Rig::connected();
    let probe = rig.send(Event::Request { id: 1, request: Request::SendGcode("g38.2 z-10".into()) });
    assert_eq!(written(&probe), ["G38.2 Z-10"]);
    assert!(rig.machine.snapshot().probing);
    let jog = rig.act_with("move", json!("right"), json!("10"));
    assert!(written(&jog).is_empty() && !alerts(&jog).is_empty(), "a jog waits for the probe");
    rig.rx("z axis zeroed");
    rig.rx("ok");
    assert!(!rig.machine.snapshot().probing);
}
