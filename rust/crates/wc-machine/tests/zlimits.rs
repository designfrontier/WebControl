//! Z travel limits: 51.292's `$63`/`$64`, mirrored from what the controller reports, refuse every Z move WebControl
//! writes past them, in mm, whatever the units or positioning; a change of zero reads them again with B20.

mod rig;

use rig::{Rig, alerts, settings, written};
use serde_json::{Value, json};
use wc_machine::Event;
use wc_machine::files::Request;

const LOWER: &str = "would move Z to -15.00 mm, below the Z lower limit of -10.00 mm.";

fn reporting(changes: &[(&str, &str, Value)], upper: &str, lower: &str) -> Rig {
    let mut rig = Rig::with(settings(changes));
    rig.boot_reporting("51.29", &[format!("$63={upper}"), format!("$64={lower}")]);
    rig.clear_written();
    rig.take_effects();
    rig
}

/// 51.292 reporting limits of 20 and -10 mm, with a status report at Z 0 (taken as up to 0.01 higher).
fn limited_with(changes: &[(&str, &str, Value)]) -> Rig {
    reporting(changes, "20.00000000", "-10.00000000")
}

fn limited() -> Rig {
    limited_with(&[])
}

fn typed(rig: &mut Rig, line: &str) -> Vec<String> {
    rig.send(Event::Request { id: 1, request: Request::SendGcode(line.into()) });
    rig.run_to_quiet()
}

fn raised_alerts(rig: &mut Rig) -> Vec<String> {
    alerts(&rig.take_effects())
}

#[test]
fn a_z_jog_past_a_limit_is_refused_and_positioning_is_put_back() {
    let mut rig = limited();
    rig.act_with("moveZ", json!("lower"), json!(5));
    assert_eq!(rig.run_to_quiet(), ["G21", "G91", "G00 Z-5.0", "G90"]);
    rig.take_effects();
    rig.act_with("moveZ", json!("lower"), json!(10));
    assert_eq!(rig.run_to_quiet(), ["G21", "G91", "G90"], "the move is dropped, absolute positioning restored");
    assert_eq!(
        raised_alerts(&mut rig),
        ["That command would move Z to -14.99 mm, below the Z lower limit of -10.00 mm."]
    );
    rig.act_with("moveZ", json!("raise"), json!(30));
    assert_eq!(rig.run_to_quiet(), ["G21", "G91", "G90"]);
    assert!(raised_alerts(&mut rig)[0].contains("above the Z upper limit of 20.00 mm"));
}

#[test]
fn a_typed_g91_stays_when_a_move_after_it_is_refused() {
    let mut rig = limited();
    assert_eq!(typed(&mut rig, "G91"), ["G91"]);
    assert!(typed(&mut rig, "G0 Z-30").is_empty(), "nothing queued would have restored G90");
    assert_eq!(raised_alerts(&mut rig).len(), 1);
    assert_eq!(typed(&mut rig, "G0 Z-1"), ["G0 Z-1"]);
}

#[test]
fn a_jog_in_inches_is_checked_in_millimetres() {
    let mut rig = limited_with(&[
        ("Computed Settings", "units", json!("INCHES")),
        ("Computed Settings", "unitsZ", json!("INCHES")),
    ]);
    rig.act_with("moveZ", json!("lower"), json!(0.5));
    assert_eq!(rig.run_to_quiet(), ["G20", "G91", "G90"]);
    assert!(raised_alerts(&mut rig)[0].contains("would move Z to -12.45 mm"), "from the report's upper bound");
    rig.act_with("moveZ", json!("lower"), json!(0.25));
    assert_eq!(rig.run_to_quiet(), ["G20", "G91", "G00 Z-0.25", "G90"]);
}

#[test]
fn typed_lines_and_macros_are_checked_in_their_units_and_positioning() {
    let mut rig = limited();
    assert_eq!(typed(&mut rig, "G0 Z-8"), ["G0 Z-8"]);
    assert!(typed(&mut rig, "G91 G0 Z-4").is_empty());
    assert_eq!(
        raised_alerts(&mut rig),
        ["That command would move Z to -12.00 mm, below the Z lower limit of -10.00 mm."]
    );
    assert!(typed(&mut rig, "G20 G0 Z1").is_empty(), "25.4 mm is above 20");
    assert_eq!(typed(&mut rig, "G20 G0 Z0.5"), ["G20 G0 Z0.5"]);
    assert_eq!(typed(&mut rig, "G21"), ["G21"]);
    assert_eq!(typed(&mut rig, "G0 Z20"), ["G0 Z20"], "exactly at a limit is allowed");
    assert_eq!(typed(&mut rig, "G28 Z0"), ["G28 Z0"], "a Z WebControl cannot follow is left to the firmware");
    let mut with_macro = limited_with(&[("Maslow Settings", "macro1", json!("G0 Z-30"))]);
    with_macro.act("macro1");
    assert!(with_macro.run_to_quiet().is_empty());
    assert_eq!(
        raised_alerts(&mut with_macro),
        ["That command would move Z to -30.00 mm, below the Z lower limit of -10.00 mm."]
    );
}

#[test]
fn without_reported_limits_nothing_is_checked() {
    let mut rig = Rig::connected();
    rig.clear_written();
    assert_eq!(typed(&mut rig, "G0 Z-300"), ["G0 Z-300"]);
    rig.act("defineZ0");
    assert_eq!(rig.run_to_quiet(), ["G10 Z0"], "no B20 on a firmware that keeps no limits");
    let mut cleared = reporting(&[], "9999.00000000", "nan");
    assert_eq!(typed(&mut cleared, "G0 Z-300"), ["G0 Z-300"]);
}

#[test]
fn a_change_of_zero_reads_the_limits_again() {
    let mut rig = limited();
    typed(&mut rig, "G0 Z-5");
    rig.act("defineZ0");
    assert_eq!(rig.run_to_quiet(), ["G10 Z0", "B20"]);
    assert_eq!(typed(&mut rig, "G0 Z-14"), ["G0 Z-14"], "unknown until B20 answers: the firmware clamps");
    rig.rx("Z Axis Upper Limit: 25.00 Lower Limit: -5.00");
    assert!(typed(&mut rig, "G0 Z-6").is_empty());
    assert!(raised_alerts(&mut rig)[0].contains("below the Z lower limit of -5.00 mm"));
    typed(&mut rig, "G20");
    rig.rx("Z Axis Upper Limit: 1.00 Lower Limit: NAN");
    assert!(typed(&mut rig, "G0 Z1.1").is_empty(), "B20 prints the controller's units");
    assert_eq!(typed(&mut rig, "G0 Z-3"), ["G0 Z-3"], "NAN is no limit");
}

#[test]
fn saving_a_limit_takes_the_controllers_reply() {
    let mut rig = Rig::with(settings(&[]));
    rig.boot_reporting("51.29", &[]);
    rig.clear_written();
    assert_eq!(typed(&mut rig, "G0 Z-30"), ["G0 Z-30"]);
    rig.rx("new zaxis lower limit=-20.00");
    assert!(typed(&mut rig, "G0 Z-21").is_empty());
}

#[test]
fn touch_z_reads_the_limits_after_its_touch_and_the_firmware_stops_it_at_the_lower_one() {
    let mut rig = limited_with(&[("Advanced Settings", "maxTouchProbePlungeDistance", json!("25"))]);
    typed(&mut rig, "G0 Z4");
    assert_eq!(written(&rig.act("touchZ")), ["G90 G38.2 Z-25"]);
    rig.rx("z axis zeroed");
    let mut after = rig.rx("ok");
    after.extend(rig.advance(100));
    assert_eq!(written(&after), ["B20"]);
}

#[test]
fn a_buffered_job_streams_nothing_after_a_change_of_zero_until_b20_answers() {
    let mut rig = limited_with(&[("Maslow Settings", "bufferOn", json!(1))]);
    rig.machine.load_gcode("G21\nG10 Z0\nG1 Z-8 F100\nG0 Z5\n");
    let mut sent = rig.act("startRun");
    sent.extend(rig.advance(500));
    assert_eq!(written(&sent), ["G21", "G10 Z0"]);
    let mut acked = rig.rx("ok");
    acked.extend(rig.rx("ok"));
    acked.extend(rig.advance(500));
    assert_eq!(written(&acked), ["B20"]);
    let mut answered = rig.rx("Z Axis Upper Limit: 15.00 Lower Limit: -5.00");
    answered.extend(rig.advance(500));
    assert!(written(&answered).is_empty(), "the job waits for B20's ok");
    let mut after = rig.rx("ok");
    after.extend(rig.advance(500));
    assert!(written(&after).is_empty());
    assert!(
        raised_alerts(&mut rig)[0].starts_with("Line 3 would move Z to -8.00 mm, below the Z lower limit of -5.00")
    );
}

#[test]
fn a_limit_printed_to_two_decimals_refuses_no_move_the_firmware_allows() {
    let mut rig = limited_with(&[
        ("Computed Settings", "units", json!("INCHES")),
        ("Computed Settings", "unitsZ", json!("INCHES")),
    ]);
    rig.rx("new zaxis lower limit=-10.41");
    assert_eq!(typed(&mut rig, "G0 Z-0.41"), ["G0 Z-0.41"], "$64=-10.414 prints as -10.41");
    assert!(typed(&mut rig, "G0 Z-0.42").is_empty());
    rig.act("defineZ0");
    rig.run_to_quiet();
    rig.rx("Z Axis Upper Limit: 0.50 Lower Limit: -0.41");
    assert_eq!(typed(&mut rig, "G0 Z-0.414"), ["G0 Z-0.414"], "B20 in inches is within 0.005 in");
    assert!(typed(&mut rig, "G0 Z-0.416").is_empty());
}

#[test]
fn a_reboot_forgets_the_limits_until_its_report() {
    let mut rig = limited();
    ["Grbl v1.00", "ready", "ok"].iter().for_each(|line| {
        rig.rx(line);
    });
    rig.answer_until_quiet("51.27");
    rig.rx("<Idle,MPos:0.00,0.00,0.00,WPos:0.000,0.000,0.000>");
    rig.advance(100);
    assert_eq!(typed(&mut rig, "G0 Z-30"), ["G0 Z-30"]);
}

#[test]
fn home_with_a_safe_height_above_the_upper_limit_raises_nothing_and_does_not_travel() {
    let mut rig = reporting(&[], "3.00000000", "-10.00000000");
    rig.act("home");
    assert_eq!(rig.run_to_quiet(), ["G90"]);
    assert_eq!(raised_alerts(&mut rig), ["That command would move Z to 5.00 mm, above the Z upper limit of 3.00 mm."]);
}

#[test]
fn a_job_pauses_before_a_line_past_a_limit() {
    let mut rig = limited();
    rig.machine.load_gcode("G21\nG0 Z5\nG1 Z-5 F100\nG1 Z-15\nG0 Z5\n");
    rig.act("startRun");
    assert_eq!(rig.run_to_quiet(), ["G21", "G0 Z5", "G1 Z-5 F100"]);
    let snapshot = rig.machine.snapshot();
    assert_eq!((snapshot.upload_flag, snapshot.gcode_index), (2, 3));
    assert_eq!(
        raised_alerts(&mut rig),
        [format!("Line 4 {LOWER} Resume stops here again: stop the job, or set the Z limits again if they are wrong.")]
    );
}

#[test]
fn a_relative_line_in_inches_is_checked_from_the_commanded_z() {
    let mut rig = limited();
    rig.machine.load_gcode("G20\nG0 Z0.1\nG91\nG1 Z-0.4 F10\nG1 Z-0.4\n");
    rig.act("startRun");
    assert_eq!(rig.run_to_quiet(), ["G20", "G0 Z0.1", "G91", "G1 Z-0.4 F10"]);
    assert!(raised_alerts(&mut rig)[0].starts_with("Line 5 would move Z to -17.78 mm"));
}

#[test]
fn a_restart_past_a_limit_pauses_the_job_before_it_travels() {
    let mut rig = reporting(&[], "3.00000000", "-10.00000000");
    rig.machine.load_gcode("G21\nG0 Z1\nG1 X10 F100\nG1 X20\n");
    rig.act_with("moveGcodeGoto", json!(3), json!(""));
    rig.act("startRun");
    let sent = rig.run_to_quiet();
    assert!(sent.iter().all(|line| !line.contains('X')), "nothing travels: {sent:?}");
    assert!(raised_alerts(&mut rig).iter().any(|alert| alert.contains("above the Z upper limit of 3.00 mm")));
    assert_eq!(rig.machine.snapshot().upload_flag, 2);
}

#[test]
fn a_probe_in_a_file_past_the_lower_limit_is_left_to_the_firmware() {
    let mut rig = limited();
    rig.machine.load_gcode("G21\nG0 Z5\nG38.2 Z-20 F100\n");
    rig.act("startRun");
    assert_eq!(rig.run_to_quiet(), ["G21", "G0 Z5", "G38.2 Z-20 F100"]);
    assert!(raised_alerts(&mut rig).iter().all(|alert| !alert.contains("limit")));
}
