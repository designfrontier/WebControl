use std::time::Duration;

use serde_json::{Value, json};
use wc_machine::{Effect, Event, Machine, Snapshot};
use wc_protocol::main::{LinkStatus, PauseButton};
use wc_protocol::{Action, Namespace, Push};
use wc_testkit::contract::Contract;

fn machine() -> Machine {
    Machine::new("10.0.0.5:5000")
}

fn action(command: &str) -> Action {
    Action { command: command.into(), arg: json!(""), arg1: json!("") }
}

fn pushes(effects: &[Effect]) -> Vec<&Push> {
    effects.iter().filter_map(|e| if let Effect::Push(p) = e { Some(p) } else { None }).collect()
}

/// Each push validated against the contract, as (command, decoded data).
fn decoded(effects: &[Effect]) -> Vec<(String, Value)> {
    pushes(effects)
        .into_iter()
        .map(|push| {
            Contract::shared().validate_frame(&push.frame().encode()).unwrap();
            let data = serde_json::from_str(push.data["data"].as_str().unwrap()).unwrap();
            (push.data["command"].as_str().unwrap().to_string(), data)
        })
        .collect()
}

#[test]
fn a_new_machine_is_idle_and_disconnected() {
    let snapshot = machine().snapshot();
    assert_eq!(
        snapshot,
        Snapshot {
            link: LinkStatus::Disconnected,
            upload_flag: 0,
            job_in_progress: false,
            manual_z_axis_adjust: false,
            pause_button: PauseButton::Pause,
            controller_firmware_version: 0.0,
            controller_board: None,
            firmware_build: None,
            host_address: "10.0.0.5:5000".into(),
        }
    );
}

#[test]
fn a_client_connecting_hears_the_host_address_then_the_link_state() {
    let effects = machine().handle(Event::ClientConnected { sid: "s1".into() }, Duration::ZERO);
    let messages = decoded(&effects);
    assert_eq!(messages[0], ("hostAddress".into(), json!({"hostAddress": "10.0.0.5:5000"})));
    assert_eq!(messages[1].0, "connectionStatus");
    assert_eq!(messages[1].1["status"], "disconnected");
    assert!(pushes(&effects).iter().all(|p| p.client.is_none() && p.ns == Namespace::MaslowCnc));
}

#[test]
fn actions_not_implemented_yet_alert_like_python() {
    let effects = machine().handle(Event::Action(action("home")), Duration::ZERO);
    let messages = decoded(&effects);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].1["message"], "Function not currently implemented.. Sorry.[home]");
    assert_eq!(messages[0].1["modalType"], "alert");
}

#[test]
fn stop_while_disconnected_writes_nothing_and_says_nothing() {
    let mut machine = machine();
    assert!(machine.handle(Event::Stop(action("stopRun")), Duration::ZERO).is_empty());
    assert!(machine.handle(Event::Stop(action("stopZ")), Duration::ZERO).is_empty());
}

#[test]
fn controller_lines_and_ticks_change_nothing_while_disconnected() {
    let mut machine = machine();
    let before = machine.snapshot();
    assert!(machine.handle(Event::ControllerLine("ok".into()), Duration::from_millis(5)).is_empty());
    assert!(machine.handle(Event::Tick, Duration::from_millis(10)).is_empty());
    assert_eq!(machine.snapshot(), before);
}

#[test]
fn the_projection_derives_the_wire_flags_from_the_run_state() {
    let snapshot = machine().snapshot();
    let state = snapshot.state_fields();
    assert_eq!(
        Value::Object(state),
        json!({"uploadFlag": 0, "jobInProgress": false, "manualZAxisAdjust": false, "pauseButtonSetting": "Pause",
            "controllerFirmwareVersion": 0.0, "controllerBoard": null, "firmwareBuild": null})
    );
}
