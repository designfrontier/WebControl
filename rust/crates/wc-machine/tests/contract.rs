//! The contract test for M1's items (plan §3.2): every message the link produces validates against its closed
//! schema, each M1 action is refused while streaming exactly when its gate says, and every client event decodes.

mod rig;

use rig::{Rig, alerts, settings};
use serde_json::{Value, json};
use wc_machine::{Effect, Event};
use wc_protocol::{ClientEvent, Frame};
use wc_testkit::contract::Contract;

const M1_MESSAGES: [&str; 13] = [
    "activateModal",
    "alarm",
    "clearAlarm",
    "connectionStatus",
    "controllerMessage",
    "errorValueMessage",
    "healthMessage",
    "homePositionMessage",
    "hostAddress",
    "positionMessage",
    "requestedSetting",
    "statusMessage",
    "updatePorts",
];

const M1_ACTIONS: [&str; 24] = [
    "move",
    "moveTo",
    "moveZ",
    "home",
    "returnToCenter",
    "moveToDefault",
    "defineZ0",
    "touchZ",
    "stopRun",
    "stopZ",
    "statusRequest",
    "reportSettings",
    "updatePorts",
    "macro1",
    "macro2",
    "clearLogs",
    "setMaxZ",
    "setMinZ",
    "clearZ",
    "getZlimits",
    "defineHome",
    "resetHomeToCenter",
    "shutdown",
    "TurnOffRPI",
];
const SENDING: &str = "Cannot issue command while sending gcode.";

#[test]
fn every_m1_message_is_produced_and_fits_its_closed_schema() {
    let contract = Contract::load();
    let mut rig = Rig::with(settings(&[("WebControl Settings", "computedPosition", json!(1))]));
    let mut effects = rig.send(Event::ClientConnected { sid: "s1".into() });
    effects.extend(rig.open());
    ["PCB v1.6 TLE9201  Detected", "Grbl v1.00", "ready", "ok", "Firmware Version 51.27"]
        .iter()
        .for_each(|line| effects.extend(rig.rx(line)));
    effects.extend(rig.rx("<Idle,MPos:1.00,2.00,0.00,WPos:0.000,0.000,0.000>"));
    effects.extend(rig.rx("[PE:0.5,-1.0,120,1650.0,1651.0]"));
    effects.extend(rig.rx("ALARM: Position Lost"));
    effects.extend(rig.send(Event::Health { cpu: 1.5 }));
    effects.extend(rig.send(Event::SettingRequest { section: "Computed Settings".into(), setting: "units".into() }));
    effects.extend(rig.send(Event::SettingRequest { section: "".into(), setting: "homePosition".into() }));
    effects.extend(rig.send(Event::Ports(Vec::new())));
    effects.extend(rig.act("upgradeHoleyFirmware"));
    effects.iter().for_each(|effect| {
        if let Effect::Push(push) = effect {
            contract.validate_frame(&push.frame().encode()).unwrap();
        }
    });
    let seen = contract.seen_commands();
    let missing: Vec<&str> = M1_MESSAGES.into_iter().filter(|c| !seen.contains(*c)).collect();
    assert!(missing.is_empty(), "never produced: {missing:?}");
}

#[test]
fn m1_actions_are_refused_while_streaming_exactly_as_their_gates_say() {
    let actions = Contract::shared().actions();
    M1_ACTIONS.iter().for_each(|command| {
        let gate = actions.get(*command).unwrap_or_else(|| panic!("{command} is not a contract action"));
        let mut rig = Rig::connected();
        rig.machine.load_gcode("G0 X1\nG0 X2\nG0 X3\n");
        rig.act("startRun");
        let refused = alerts(&rig.act(command)).contains(&SENDING.to_string());
        assert_eq!(refused, gate["allowedDuring"] != "always", "{command} ({})", gate["allowedDuring"]);
        let mut idle = Rig::connected();
        assert!(!alerts(&idle.act(command)).contains(&SENDING.to_string()), "{command} while idle");
    });
}

fn sample(event: &str) -> Value {
    match event {
        "action" => json!({"data": {"command": "home", "arg": "", "arg1": ""}}),
        "settingRequest" => json!({"data": {"section": "Maslow Settings", "setting": "bedWidth"}}),
        _ => json!({"data": {}}),
    }
}

#[test]
fn every_client_event_decodes_and_m1_handles_action_and_setting_request() {
    Contract::shared().client_events("/MaslowCNC").iter().for_each(|event| {
        let frame = Frame { ns: "/MaslowCNC".into(), event: event.clone(), data: sample(event) };
        let decoded = ClientEvent::from_frame(&frame).unwrap_or_else(|| panic!("{event} does not decode"));
        let handled = !matches!(decoded, ClientEvent::Unhandled(_));
        assert_eq!(handled, event == "action" || event == "settingRequest", "{event}");
    });
}
