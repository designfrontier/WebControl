//! R arcs reach every firmware as I and J, G91 arcs with absolute X and Y and G90.1 arcs with I and J offsets, and
//! G90.1 and G91.1 never reach it (spec/firmware.md §9, B19): typed, streamed, shifted to home and restarted
//! mid-file; an arc that cannot be rewritten, or whose start only a status report gives too roughly, is refused.

mod rig;

use rig::{Rig, alerts, settings, written};
use serde_json::{Value, json};
use wc_machine::Event;
use wc_machine::files::Request;

fn typed(rig: &mut Rig, text: &str) -> Vec<String> {
    rig.send(Event::Request { id: 1, request: Request::SendGcode(text.into()) });
    rig.run_to_quiet()
}

fn job(rig: &mut Rig, lines: &[&str]) {
    rig.machine.load_gcode(&lines.join("\n"));
}

#[test]
fn a_typed_r_arc_goes_to_51_27_with_i_and_j_from_the_reported_position() {
    let mut rig = Rig::connected();
    assert_eq!(typed(&mut rig, "g2 x10 y10 r10"), ["G2 X10 Y10 I10.0000 J0.0000"]);
    assert_eq!(typed(&mut rig, "G3 X0 Y0 R-10"), ["G3 X0 Y0 I-10.0000 J0.0000"]);
    assert_eq!(typed(&mut rig, "G91\nG2 X10 Y10 R10\nG90"), ["G91", "G2 X10.0000 Y10.0000 I10.0000 J0.0000", "G90"]);
}

#[test]
fn a_typed_lower_case_r_arc_is_converted() {
    let mut rig = Rig::connected();
    assert_eq!(typed(&mut rig, "g0 x0 y0\ng2 x10 y0 r5"), ["G0 X0 Y0", "G2 X10 Y0 I5.0000 J0.0000"]);
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
fn a_typed_arc_with_no_centre_is_refused_with_an_alert() {
    let mut rig = Rig::connected();
    let mut effects = rig.send(Event::Request { id: 1, request: Request::SendGcode("G2 X10 Y0\nG0 X1".into()) });
    (0..10).for_each(|_| effects.extend(rig.advance(100)));
    assert_eq!(
        alerts(&effects),
        [
            "That command is an arc with no centre (I or J) and no radius (R); the controller would move straight to its \
          end at full speed."
        ]
    );
    assert_eq!(written(&effects), ["G0 X1"]);
}

#[test]
fn a_job_pauses_before_an_arc_in_the_g18_plane() {
    let mut rig = Rig::connected();
    job(&mut rig, &["G21", "G0 X0 Y0", "G18", "G2 X10 Z-5 I5 K0", "G0 X1"]);
    rig.act("startRun");
    let mut effects = Vec::new();
    (0..10).for_each(|_| {
        effects.extend(rig.advance(100));
        effects.extend(rig.rx("ok"));
    });
    assert_eq!(written(&effects), ["G0 X0 Y0", "G18"]);
    assert_eq!(
        alerts(&effects),
        ["Line 4 is an arc in the G18 or G19 plane; the controller ignores the plane and would cut it in X and Y. \
          Resume stops here again: stop the job, fix the file and load it again."]
    );
    assert_eq!(rig.machine.snapshot().gcode_index, 3);
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
    assert_eq!(typed(&mut rig, "G2 X10 Y0 R5"), ["G2 X10 Y0 I0.0000 J-5.0000"]);
}

#[test]
fn after_a_stop_a_half_circle_the_reports_two_decimals_could_move_is_refused_until_an_absolute_move() {
    let mut rig = Rig::connected();
    rig.act("stopRun");
    rig.advance(300);
    rig.rx("<Idle,MPos:5.00,5.00,0.00,WPos:0.000,0.000,0.000>");
    rig.clear_written();
    let mut effects = rig.send(Event::Request { id: 1, request: Request::SendGcode("G2 X15 Y5 R5".into()) });
    (0..10).for_each(|_| effects.extend(rig.advance(100)));
    assert!(written(&effects).is_empty(), "{:?}", written(&effects));
    assert!(alerts(&effects)[0].starts_with("That command is an arc the controller needs rewritten, and WebControl"));
    assert!(alerts(&effects)[0].contains("two decimals"));
    assert_eq!(typed(&mut rig, "G0 X5 Y5\nG2 X15 Y5 R5"), ["G0 X5 Y5", "G2 X15 Y5 I5.0000 J0.0000"]);
}

fn inches() -> Rig {
    Rig::connected_with(settings(&[("Computed Settings", "units", json!("INCHES"))]))
}

#[test]
fn a_job_pauses_before_a_near_half_circle_from_a_reported_start_and_resume_converts_it_from_the_restart() {
    let mut rig = inches();
    job(&mut rig, &["G20", "G2 X1 Y0 R0.5", "G0 X0"]);
    rig.clear_written();
    let mut effects = rig.act("startRun");
    effects.extend(rig.advance(100));
    assert_eq!(rig.run_to_quiet(), ["G20"]);
    effects.extend(rig.take_effects());
    let uncertain = "Line 2 is an arc the controller needs rewritten, and WebControl knows where the sled is only";
    let alert = alerts(&effects).into_iter().find(|alert| alert.starts_with(uncertain));
    assert!(
        alert.is_some_and(
            |alert| alert.ends_with("Resume returns the sled to the file's position and tries the line again.")
        ),
        "{effects:?}"
    );
    assert_eq!((rig.machine.snapshot().upload_flag, rig.machine.snapshot().gcode_index), (2, 1));
    rig.act("resumeRun");
    let sent = rig.run_to_quiet();
    assert!(sent.contains(&"G0 X0.0 Y0.0".to_string()), "{sent:?}");
    assert_eq!(sent[sent.len() - 2..], ["G2 X1 Y0 I0.5000 J0.0000", "G0 X0"]);
}

#[test]
fn a_job_that_places_the_sled_first_converts_a_near_half_circle_through_a_pause_and_resume() {
    let mut rig = inches();
    job(&mut rig, &["G20", "G0 X0 Y0", "G1 X0.5", "G2 X1.5 Y0 R0.5", "G0 X0"]);
    rig.clear_written();
    rig.act("startRun");
    rig.act("pauseRun");
    let mut sent = rig.run_to_quiet();
    rig.act("resumeRun");
    sent.extend(rig.run_to_quiet());
    let effects = rig.take_effects();
    assert!(alerts(&effects).is_empty(), "{:?}", alerts(&effects));
    assert_eq!(sent[sent.len() - 2..], ["G2 X1.5 Y0 I0.5000 J0.0000", "G0 X0"], "{sent:?}");
}

#[test]
fn a_typed_g90_1_arc_is_written_with_offsets_and_the_mode_words_are_never_sent() {
    let mut rig = Rig::connected();
    assert_eq!(typed(&mut rig, "G91.1"), Vec::<String>::new());
    assert_eq!(
        typed(&mut rig, "G0 X10 Y10\nG90.1\nG2 X20 Y0 I10 J0\ng91.1 g1 x1\nG2 X10 Y-10 I-10 J0"),
        ["G0 X10 Y10", "G2 X20 Y0 I0.0000 J-10.0000", "G1 X1", "G2 X10 Y-10 I-10 J0"]
    );
}

#[test]
fn a_vcarve_file_starting_g91_1_streams_its_moves_absolute_without_it() {
    let home: [(&str, &str, Value); 2] =
        [("Advanced Settings", "homeX", json!(10.0)), ("Advanced Settings", "homeY", json!(-5.0))];
    let mut rig = Rig::connected_with(settings(&home));
    job(&mut rig, &VCARVE);
    rig.act("startRun");
    assert_eq!(
        rig.run_to_quiet(),
        [
            "G00 G21 G17 G90 G40 G49 G80",
            "G71",
            "G00 X10.000 Y-5.000 F1500.0",
            "G1 Z-3.000 F300.0",
            "G1 X130.500 Y7.000 F1500.0",
            "G2 X120.500 Y17.000 I-10.000 J0.000",
            "G1 X22.000 Y17.000",
            "G0 Z5.000",
        ]
    );
    assert_eq!(rig.machine.snapshot().gcode_index, 0);
}

const VCARVE: [&str; 9] = [
    "G00G21G17G90G40G49G80",
    "G71G91.1",
    "G00X0.000Y0.000F1500.0",
    "G1Z-3.000F300.0",
    "G91.1",
    "G1X120.500Y12.000F1500.0",
    "G2X110.500Y22.000I-10.000J0.000",
    "G1X12.000Y22.000",
    "G0Z5.000",
];

#[test]
fn play_mid_file_in_a_vcarve_file_returns_to_the_files_absolute_position() {
    let mut rig = Rig::connected();
    job(&mut rig, &VCARVE);
    rig.act_with("moveGcodeGoto", json!(6), json!(""));
    rig.act("startRun");
    let sent = rig.run_to_quiet();
    assert!(sent.contains(&"G0 X120.5 Y12.0".to_string()), "{sent:?}");
    assert!(sent.iter().all(|line| !line.contains("91") && !line.contains("90.1")), "{sent:?}");
    assert_eq!(sent[sent.len() - 3..], ["G2 X110.500 Y22.000 I-10.000 J0.000", "G1 X12.000 Y22.000", "G0 Z5.000"]);
}

const ABSOLUTE_CENTRES: [&str; 7] = ["G21", "G90", "G90.1", "G0 X10 Y10", "G1 Z-1 F100", "G2 X20 Y0 I10 J0", "G0 Z5"];

#[test]
fn a_g90_1_file_has_its_centres_shifted_to_home_and_written_as_offsets() {
    let home: [(&str, &str, Value); 2] =
        [("Advanced Settings", "homeX", json!(10.0)), ("Advanced Settings", "homeY", json!(-5.0))];
    let mut rig = Rig::connected_with(settings(&home));
    job(&mut rig, &ABSOLUTE_CENTRES);
    rig.act("startRun");
    assert_eq!(rig.run_to_quiet(), ["G21", "G90", "G0 X20 Y5", "G1 Z-1 F100", "G2 X30 Y-5 I0.0000 J-10.0000", "G0 Z5"]);
}

#[test]
fn play_mid_file_in_a_g90_1_file_keeps_converting_its_centres() {
    let mut rig = Rig::connected();
    job(&mut rig, &ABSOLUTE_CENTRES);
    rig.act_with("moveGcodeGoto", json!(5), json!(""));
    rig.act("startRun");
    let sent = rig.run_to_quiet();
    assert!(sent.contains(&"G0 X10.0 Y10.0".to_string()), "{sent:?}");
    assert_eq!(sent[sent.len() - 2..], ["G2 X20 Y0 I0.0000 J-10.0000", "G0 Z5"]);
}

#[test]
fn a_g90_1_from_an_earlier_run_is_forgotten_by_the_next_run_and_by_a_restart_before_it() {
    let mut rig = Rig::connected();
    let lines = ["G21", "G90", "G0 X10 Y10", "G2 X20 Y0 I0 J-10", "G90.1", "G2 X30 Y10 I20 J10", "G0 X10 Y10"];
    job(&mut rig, &lines);
    rig.act("startRun");
    assert_eq!(rig.run_to_quiet()[4], "G2 X30 Y10 I0.0000 J10.0000");
    rig.act("startRun");
    assert_eq!(rig.run_to_quiet()[3], "G2 X20 Y0 I0 J-10");
    rig.act_with("moveGcodeGoto", json!(3), json!(""));
    rig.act("startRun");
    let sent = rig.run_to_quiet();
    assert!(sent.contains(&"G2 X20 Y0 I0 J-10".to_string()), "{sent:?}");
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

#[test]
fn a_lower_case_file_has_its_r_arc_converted_and_its_g91_arc_written_absolute() {
    let mut rig = Rig::connected();
    job(&mut rig, &["g21", "g90", "g0 x0 y0", "g2 x10 y0 r5", "g91", "g2 x10 y0 i5 j0", "g90", "g0 x0"]);
    rig.act("startRun");
    assert_eq!(
        rig.run_to_quiet(),
        ["G21", "G90", "G0 X0 Y0", "G2 X10 Y0 I5.0000 J0.0000", "G91", "G2 X20.0000 Y0.0000 I5 J0", "G90", "G0 X0"]
    );
}

#[test]
fn play_mid_file_in_a_lower_case_g91_file_writes_the_arc_from_the_restart() {
    let mut rig = Rig::connected();
    job(&mut rig, &["g21", "g0 x10 y10", "g91", "g1 z-1 f100", "g2 x10 y-10 i0 j-10", "g1 x1"]);
    rig.act_with("moveGcodeGoto", json!(4), json!(""));
    rig.act("startRun");
    let sent = rig.run_to_quiet();
    assert!(sent.contains(&"G0 X10.0 Y10.0".to_string()), "{sent:?}");
    assert_eq!(sent[sent.len() - 2..], ["G2 X20.0000 Y0.0000 I0 J-10", "G1 X1"]);
}

#[test]
fn play_mid_file_is_refused_at_a_lower_case_g91_arc_no_absolute_move_places() {
    let mut rig = Rig::connected();
    job(&mut rig, &["g21", "g91", "g1 z-1 f100", "g2 x10 y-10 i0 j-10", "g90", "g0 x0 y0"]);
    rig.act_with("moveGcodeGoto", json!(3), json!(""));
    rig.clear_written();
    let mut effects = rig.act("startRun");
    (0..10).for_each(|_| effects.extend(rig.advance(100)));
    assert!(written(&effects).is_empty(), "{:?}", written(&effects));
    assert!(alerts(&effects).iter().any(|alert| alert.starts_with("Cannot return to this line")), "{effects:?}");
    assert_eq!(rig.machine.snapshot().gcode_index, 3);
}
