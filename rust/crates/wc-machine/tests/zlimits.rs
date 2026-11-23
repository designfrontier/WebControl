//! Z travel limits: every Z move WebControl writes is checked against them, in mm, whatever the units or
//! positioning, and Define Z0, Touch Z and G10 move them so they stay put on the Z mechanism.

mod rig;

use rig::{Rig, alerts, settings, written};
use serde_json::{Value, json};
use wc_machine::Event;
use wc_machine::files::Request;

const LOWER: &str = "would move Z to -15.00 mm, below the Z lower limit of -10.00 mm.";

fn limited_with(changes: &[(&str, &str, Value)]) -> Rig {
    let limits =
        [("Maslow Settings", "zAxisUpperLimit", json!("20")), ("Maslow Settings", "zAxisLowerLimit", json!("-10"))];
    let all: Vec<(&str, &str, Value)> = limits.into_iter().chain(changes.iter().cloned()).collect();
    let mut rig = Rig::connected_with(settings(&all));
    rig.clear_written();
    rig
}

/// Limits of 20 and -10 mm, with a status report at Z 0 (taken as up to 0.01 higher).
fn limited() -> Rig {
    limited_with(&[])
}

/// Connected with the limits, before any status report told where Z is.
fn unreported(changes: &[(&str, &str, Value)]) -> Rig {
    let limits =
        [("Maslow Settings", "zAxisUpperLimit", json!("20")), ("Maslow Settings", "zAxisLowerLimit", json!("-10"))];
    let all: Vec<(&str, &str, Value)> = limits.into_iter().chain(changes.iter().cloned()).collect();
    let mut rig = Rig::with(settings(&all));
    rig.open();
    ["Grbl v1.00", "ready", "ok"].iter().for_each(|line| {
        rig.rx(line);
    });
    rig.run_to_quiet();
    rig.take_effects();
    rig
}

fn typed(rig: &mut Rig, line: &str) -> Vec<String> {
    rig.send(Event::Request { id: 1, request: Request::SendGcode(line.into()) });
    rig.run_to_quiet()
}

fn limits(rig: &Rig) -> (Value, Value) {
    let stored = rig.machine.snapshot().settings;
    let value = |key: &str| stored.value("Maslow Settings", key).cloned().unwrap_or(Value::Null);
    (value("zAxisUpperLimit"), value("zAxisLowerLimit"))
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
    assert_eq!(typed(&mut rig, "G91 G0 Z-4"), Vec::<String>::new());
    assert_eq!(
        raised_alerts(&mut rig),
        ["That command would move Z to -12.00 mm, below the Z lower limit of -10.00 mm."]
    );
    assert_eq!(typed(&mut rig, "G20 G0 Z1"), Vec::<String>::new(), "25.4 mm is above 20");
    assert_eq!(typed(&mut rig, "G20 G0 Z0.5"), ["G20 G0 Z0.5"]);
    assert_eq!(typed(&mut rig, "G21"), ["G21"]);
    assert_eq!(typed(&mut rig, "G0 Z20"), ["G0 Z20"], "exactly at a limit is allowed");
    assert_eq!(typed(&mut rig, "G0 X5"), ["G0 X5"]);
    assert_eq!(typed(&mut rig, "G28 Z0"), Vec::<String>::new());
    assert!(raised_alerts(&mut rig).last().unwrap().contains("does not know"));
    let mut with_macro = limited_with(&[("Maslow Settings", "macro1", json!("G0 Z-30"))]);
    with_macro.act("macro1");
    assert!(with_macro.run_to_quiet().is_empty());
    assert_eq!(
        raised_alerts(&mut with_macro),
        ["That command would move Z to -30.00 mm, below the Z lower limit of -10.00 mm."]
    );
}

#[test]
fn no_limits_means_no_check() {
    let mut rig = Rig::connected();
    rig.clear_written();
    assert_eq!(typed(&mut rig, "G0 Z-300"), ["G0 Z-300"]);
    assert_eq!(typed(&mut rig, "G28 Z0"), ["G28 Z0"]);
}

#[test]
fn define_z0_moves_the_limits_with_the_zero() {
    let mut rig = limited();
    typed(&mut rig, "G0 Z-5");
    rig.act("defineZ0");
    assert_eq!(rig.run_to_quiet(), ["G10 Z0"]);
    assert_eq!(limits(&rig), (json!("25.0"), json!("-5.0")));
    typed(&mut rig, "G0 Z2");
    typed(&mut rig, "G10 Z5");
    assert_eq!(limits(&rig), (json!("28.0"), json!("-2.0")), "a G10 to 5 at 2 adds 3");
}

#[test]
fn renaming_z_from_an_unknown_height_is_refused() {
    let mut rig = unreported(&[]);
    rig.act("defineZ0");
    assert!(rig.run_to_quiet().is_empty());
    assert_eq!(
        raised_alerts(&mut rig),
        ["That command moves Z from a height WebControl does not know, so the Z limits cannot be checked."]
    );
    assert_eq!(limits(&rig), (json!("20"), json!("-10")));
}

#[test]
fn touch_z_stops_at_the_lower_limit_and_its_touch_moves_the_limits() {
    let mut rig = limited_with(&[("Advanced Settings", "maxTouchProbePlungeDistance", json!("25"))]);
    typed(&mut rig, "G0 Z4");
    assert_eq!(written(&rig.act("touchZ")), ["G90 G38.2 Z-10"]);
    rig.rx("<Idle,MPos:0.00,0.00,1.00,WPos:0.000,0.000,0.000>");
    rig.rx("<Idle,MPos:0.00,0.00,-2.50,WPos:0.000,0.000,0.000>");
    rig.rx("z axis zeroed");
    rig.rx("ok");
    assert_eq!(limits(&rig), (json!("22.5"), json!("-7.5")), "the touch at -2.5 became 0");
}

#[test]
fn touch_z_within_the_limit_probes_its_full_plunge() {
    let mut rig = limited_with(&[("Advanced Settings", "maxTouchProbePlungeDistance", json!("6"))]);
    typed(&mut rig, "G0 Z4");
    assert_eq!(written(&rig.act("touchZ")), ["G90 G38.2 Z-6"]);
    rig.rx("z axis zeroed");
    rig.rx("ok");
    assert_eq!(limits(&rig), (json!("16.0"), json!("-14.0")), "no report while probing: from where it started");
}

#[test]
fn touch_z_from_an_unknown_height_is_refused() {
    let mut rig = unreported(&[]);
    rig.act("touchZ");
    assert!(rig.run_to_quiet().is_empty());
    assert_eq!(raised_alerts(&mut rig).len(), 1);
    assert!(!rig.machine.snapshot().probing);
}

#[test]
fn home_with_a_safe_height_above_the_upper_limit_raises_nothing_and_does_not_travel() {
    let mut rig = limited_with(&[("Maslow Settings", "zAxisUpperLimit", json!("3"))]);
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
        [format!(
            "Line 4 {LOWER} Resume stops here again: stop the job, or change the Z limits (Maslow Settings) if they are wrong."
        )]
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
    let mut rig = limited_with(&[("Maslow Settings", "zAxisUpperLimit", json!("3"))]);
    rig.machine.load_gcode("G21\nG0 Z1\nG1 X10 F100\nG1 X20\n");
    rig.act_with("moveGcodeGoto", json!(3), json!(""));
    rig.act("startRun");
    let sent = rig.run_to_quiet();
    assert!(sent.iter().all(|line| !line.contains('X')), "nothing travels: {sent:?}");
    assert!(raised_alerts(&mut rig).iter().any(|alert| alert.contains("above the Z upper limit of 3.00 mm")));
    assert_eq!(rig.machine.snapshot().upload_flag, 2);
}

#[test]
fn a_probe_in_a_file_past_the_lower_limit_pauses_the_job() {
    let mut rig = limited();
    rig.machine.load_gcode("G21\nG0 Z5\nG38.2 Z-20 F100\n");
    rig.act("startRun");
    assert_eq!(rig.run_to_quiet(), ["G21", "G0 Z5"]);
    assert!(raised_alerts(&mut rig)[0].starts_with("Line 3 would move Z to -20.00 mm, below the Z lower limit"));
    assert!(!rig.machine.snapshot().probing);
}

#[test]
fn the_limits_save_as_numbers_or_blank() {
    let mut rig = limited();
    let put = |rig: &mut Rig, value: &str| {
        let body = json!({"values": {"zAxisUpperLimit": value}});
        let request = wc_machine::Request::UpdateSettings { section: "maslow".into(), body };
        let effects = rig.send(Event::Edit { id: 1, request });
        effects.iter().find_map(|effect| match effect {
            wc_machine::Effect::Respond { response, .. } => Some(response.status),
            _ => None,
        })
    };
    assert_eq!(put(&mut rig, "high"), Some(400));
    assert_eq!(put(&mut rig, ""), Some(200));
    assert_eq!(limits(&rig).0, json!(""));
    assert_eq!(typed(&mut rig, "G0 Z300"), ["G0 Z300"], "no upper limit any more");
}
