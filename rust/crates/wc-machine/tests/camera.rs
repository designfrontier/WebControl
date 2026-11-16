//! The live camera overlay's actions and the capture task's reports, while optical calibration is on and off.

mod rig;

use rig::{Rig, alerts, pushed, settings};
use serde_json::{Value, json};
use wc_machine::files::Request;
use wc_machine::{CameraReport, Event};

fn optical_on() -> Rig {
    Rig::with(settings(&[("WebControl Settings", "opticalCalibrationEnabled", json!(1))]))
}

fn camera(effects: &[wc_machine::Effect]) -> Vec<Value> {
    pushed(effects, "cameraMessage")
}

/// Neither a camera message nor an alert; the first event also pushes the status, which is not the camera's.
fn quiet(effects: &[wc_machine::Effect]) -> bool {
    camera(effects).is_empty() && alerts(effects).is_empty()
}

fn status(on: &str) -> Value {
    json!({"command": "updateCamera", "data": on})
}

#[test]
fn the_camera_actions_do_nothing_while_optical_calibration_is_off() {
    let mut rig = Rig::new();
    ["toggleCamera", "queryCamera"].iter().for_each(|command| assert!(quiet(&rig.act(command))));
    assert!(quiet(&rig.act_with("statusRequest", json!("cameraStatus"), json!(""))));
    assert!(!rig.machine.snapshot().camera);
    assert!(quiet(&rig.send(Event::Camera(CameraReport::Frame("AAAA".into())))));
}

#[test]
fn toggle_turns_the_overlay_on_and_off_and_every_client_hears_it() {
    let mut rig = optical_on();
    assert_eq!(camera(&rig.act("toggleCamera")), [status("on")]);
    assert!(rig.machine.snapshot().camera);
    assert_eq!(camera(&rig.act("queryCamera")), [status("on")]);
    assert_eq!(camera(&rig.act_with("statusRequest", json!("cameraStatus"), json!(""))), [status("on")]);
    assert!(quiet(&rig.act("statusRequest")), "only cameraStatus asks about the camera");
    assert_eq!(camera(&rig.act("toggleCamera")), [status("off")]);
    assert!(!rig.machine.snapshot().camera);
    assert_eq!(camera(&rig.act("queryCamera")), [status("off")]);
}

#[test]
fn frames_reach_the_overlay_only_while_it_is_on() {
    let mut rig = optical_on();
    assert!(quiet(&rig.send(Event::Camera(CameraReport::Frame("late".into())))));
    rig.act("toggleCamera");
    let shown = rig.send(Event::Camera(CameraReport::Frame("iVBORw0KGgo=".into())));
    assert_eq!(camera(&shown), [json!({"command": "cameraImageUpdated", "data": "iVBORw0KGgo="})]);
}

#[test]
fn a_camera_that_fails_turns_the_overlay_off_with_an_alert() {
    let mut rig = optical_on();
    rig.act("toggleCamera");
    let failed = rig.send(Event::Camera(CameraReport::Failed("Camera: /dev/video0: No such device".into())));
    assert_eq!(camera(&failed), [status("off")]);
    assert_eq!(alerts(&failed), ["Camera: /dev/video0: No such device"]);
    assert!(!rig.machine.snapshot().camera);
    assert!(quiet(&rig.send(Event::Camera(CameraReport::Failed("again".into())))));
    assert_eq!(camera(&rig.act("toggleCamera")), [status("on")], "toggling tries the camera again");
}

#[test]
fn turning_optical_calibration_off_stops_the_stream() {
    let mut rig = optical_on();
    rig.act("toggleCamera");
    let off = vec![("WebControl Settings".into(), "opticalCalibrationEnabled".into(), json!(0))];
    rig.send(Event::Request { id: 1, request: Request::SetSettings(off) });
    assert!(!rig.machine.snapshot().camera);
    assert!(quiet(&rig.send(Event::Camera(CameraReport::Frame("stale".into())))));
}
