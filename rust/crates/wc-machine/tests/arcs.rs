//! R arcs reach every firmware as I and J and G91 arcs with absolute X and Y (spec/firmware.md §9, B19): typed,
//! streamed, shifted to home and restarted mid-file; an arc that cannot be rewritten is refused.

mod rig;

use rig::{Rig, alerts, settings, written};
use serde_json::{Value, json};
use wc_machine::Event;
use wc_machine::files::Request;

fn typed(rig: &mut Rig, text: &str) -> Vec<String> {
    rig.send(Event::Request { id: 1, request: Request::SendGcode(text.into()) });
    rig.run_to_quiet()
}

fn upper(lines: Vec<String>) -> Vec<String> {
    lines.into_iter().map(|line| line.to_ascii_uppercase()).collect()
}

fn job(rig: &mut Rig, lines: &[&str]) {
    rig.machine.load_gcode(&lines.join("\n"));
}

#[test]
fn a_typed_r_arc_goes_to_51_27_with_i_and_j_from_the_reported_position() {
    let mut rig = Rig::connected();
    assert_eq!(upper(typed(&mut rig, "g2 x10 y10 r10")), ["G2 X10 Y10 I10.0000 J0.0000"]);
    assert_eq!(typed(&mut rig, "G3 X0 Y0 R-10"), ["G3 X0 Y0 I-10.0000 J0.0000"]);
    assert_eq!(typed(&mut rig, "G91\nG2 X10 Y10 R10\nG90"), ["G91", "G2 X10.0000 Y10.0000 I10.0000 J0.0000", "G90"]);
}

#[test]
fn a_typed_r_arc_in_inches_is_converted_in_inches() {
    let mut rig = Rig::connected();
    assert_eq!(typed(&mut rig, "G20\nG0 X1 Y1\nG2 X2 Y0 R1"), ["G20", "G0 X1 Y1", "G2 X2 Y0 I0.0000 J-1.0000"]);
}

#[test]
fn on_51_29x_an_r_arc_is_converted_too_since_the_firmware_drops_rs_sign() {
    let mut rig = Rig::connected_on(settings(&[]), "51.29");
    assert_eq!(typed(&mut rig, "G2 X10 Y10 R-10"), ["G2 X10 Y10 I0.0000 J10.0000"]);
    let mut effects = rig.send(Event::Request { id: 1, request: Request::SendGcode("G2 X30 Y10 R1".into()) });
    (0..10).for_each(|_| effects.extend(rig.advance(100)));
    assert!(written(&effects).is_empty());
}

#[test]
fn a_typed_relative_i_j_arc_is_written_absolute_and_one_with_z_is_refused() {
    let mut rig = Rig::connected();
    assert_eq!(
        typed(&mut rig, "G0 X10 Y10\nG91\nG2 X10 Y-10 I0 J-10\nG1 X1"),
        ["G0 X10 Y10", "G91", "G2 X20.0000 Y0.0000 I0 J-10", "G1 X1"]
    );
    let mut effects = rig.send(Event::Request { id: 1, request: Request::SendGcode("G3 X1 Y1 I1 J0 Z-1".into()) });
    (0..10).for_each(|_| effects.extend(rig.advance(100)));
    assert_eq!(
        alerts(&effects),
        ["That command is a relative (G91) arc with a Z move; the controller takes an arc's Z as absolute."]
    );
    assert!(written(&effects).is_empty());
}

#[test]
fn a_typed_r_arc_no_i_and_j_can_give_is_refused_with_an_alert() {
    let mut rig = Rig::connected();
    let mut effects = rig.send(Event::Request { id: 1, request: Request::SendGcode("G2 X10 Y0 R1\nG0 X1".into()) });
    (0..10).for_each(|_| effects.extend(rig.advance(100)));
    assert_eq!(
        alerts(&effects),
        ["That command is an arc whose radius (R) is shorter than half the distance to its end."]
    );
    assert_eq!(written(&effects), ["G0 X1"]);
}

#[test]
fn an_r_arc_that_fits_the_buffer_only_with_its_r_is_refused_as_too_long() {
    let mut rig = Rig::connected();
    let line = format!("G2 X10 Y10 R10{}", " ".repeat(100));
    let mut effects = rig.send(Event::Request { id: 1, request: Request::SendGcode(line) });
    (0..10).for_each(|_| effects.extend(rig.advance(100)));
    assert_eq!(alerts(&effects), ["That command is 129 characters; the controller accepts at most 126."]);
    assert!(written(&effects).is_empty());
}

#[test]
fn after_a_stop_the_status_report_says_where_the_arc_starts() {
    let mut rig = Rig::connected();
    rig.send(Event::Request { id: 1, request: Request::SendGcode("G0 X50 Y50".into()) });
    rig.advance(100);
    rig.act("stopRun");
    rig.advance(300);
    rig.rx("<Idle,MPos:5.00,5.00,0.00,WPos:0.000,0.000,0.000>");
    rig.clear_written();
    assert_eq!(typed(&mut rig, "G2 X15 Y5 R5"), ["G2 X15 Y5 I5.0000 J0.0000"]);
}

#[test]
fn with_the_sled_lost_and_no_report_since_an_r_arc_is_refused() {
    let mut rig = Rig::connected();
    let mut effects = rig.send(Event::Request { id: 1, request: Request::SendGcode("B08\nG2 X10 Y0 R5".into()) });
    (0..10).for_each(|_| effects.extend(rig.advance(100)));
    effects.extend(rig.rx("ok"));
    (0..10).for_each(|_| effects.extend(rig.advance(100)));
    assert_eq!(written(&effects), ["B08"]);
    assert!(alerts(&effects)[0].contains("does not know where the sled is"), "{:?}", alerts(&effects));
}

#[test]
fn a_streamed_r_arc_is_converted_after_the_home_shift() {
    let home: [(&str, &str, Value); 2] =
        [("Advanced Settings", "homeX", json!(10.0)), ("Advanced Settings", "homeY", json!(-5.0))];
    let mut rig = Rig::connected_with(settings(&home));
    job(&mut rig, &["G21", "G90", "G0 X0 Y10", "G2 X10 Y0 R10 F300", "G0 X0"]);
    rig.act("startRun");
    let sent = rig.run_to_quiet();
    assert_eq!(sent, ["G21", "G90", "G0 X10 Y5", "G2 X20 Y-5 I0.0000 J-10.0000 F300", "G0 X10"]);
}

#[test]
fn play_mid_file_converts_the_r_arc_from_the_restart_position() {
    let mut rig = Rig::connected();
    job(&mut rig, &["G21", "G90", "G0 X10 Y10", "G1 Z-1 F100", "G2 X20 Y0 R10", "G0 Z5"]);
    rig.act_with("moveGcodeGoto", json!(4), json!(""));
    rig.act("startRun");
    let sent = rig.run_to_quiet();
    assert!(sent.contains(&"G0 X10.0 Y10.0".to_string()), "{sent:?}");
    assert_eq!(sent[sent.len() - 2..], ["G2 X20 Y0 I0.0000 J-10.0000", "G0 Z5"]);
}

#[test]
fn play_mid_file_in_a_g91_file_writes_the_arc_from_where_the_restart_put_the_sled() {
    let mut rig = Rig::connected();
    job(&mut rig, &["G21", "G0 X10 Y10", "G91", "G1 Z-1 F100", "G2 X10 Y-10 I0 J-10", "G1 X1"]);
    rig.act_with("moveGcodeGoto", json!(4), json!(""));
    rig.act("startRun");
    let sent = rig.run_to_quiet();
    assert!(sent.contains(&"G0 X10.0 Y10.0".to_string()), "{sent:?}");
    assert_eq!(sent[sent.len() - 2..], ["G2 X20.0000 Y0.0000 I0 J-10", "G1 X1"]);
}

#[test]
fn a_buffered_job_pauses_before_an_r_arc_it_cannot_convert() {
    let mut rig = Rig::connected_with(settings(&[("Maslow Settings", "bufferOn", json!(1))]));
    job(&mut rig, &["G21", "G0 X0 Y0", "G2 X0 Y0 R5", "G0 X1"]);
    rig.act("startRun");
    assert_eq!(rig.run_to_quiet(), ["G21", "G0 X0 Y0"]);
    assert_eq!(rig.machine.snapshot().upload_flag, 2);
    assert_eq!(rig.machine.snapshot().gcode_index, 2);
}
