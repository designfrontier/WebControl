//! The contract test for M1's, M2's, M3's and M5's items (plan §3.2): every message the machine produces validates
//! against its closed schema, each of their actions is refused while streaming exactly when its gate says, and every
//! client event decodes.

mod rig;

use rig::{Rig, alerts, settings};
use serde_json::{Value, json};
use wc_config::Settings;
use wc_machine::files::{Gcode, Origin, Request};
use wc_machine::{Effect, Event};
use wc_protocol::{ClientEvent, Frame};
use wc_testkit::contract::Contract;

const MESSAGES: [&str; 19] = [
    "boardCutDataUpdateCompressed",
    "boardDataUpdate",
    "gcodePositionMessage",
    "gcodeUpdateCompressed",
    "showFPSpinner",
    "updateDirectories",
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

const ACTIONS: [&str; 33] = [
    "clearGCode",
    "createDirectory",
    "boardProcessGCode",
    "boardClearBoard",
    "moveGcodeZ",
    "moveGcodeGoto",
    "moveGcodeIndex",
    "move",
    "moveTo",
    "moveZ",
    "home",
    "returnToCenter",
    "moveToDefault",
    "defineZ0",
    "touchZ",
    "zToZero",
    "zToSafeHeight",
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
/// M2's setup and PID actions.
const M2_ACTIONS: [&str; 12] = [
    "resetChainLengths",
    "testMotors",
    "wipeEEPROM",
    "setSprockets",
    "rotateSprocket",
    "setSprocketAutomatic",
    "setSprocketsZero",
    "setSprocketsDefault",
    "adjustChain",
    "setFakeServo",
    "executeVelocityPIDTest",
    "executePositionPIDTest",
];
/// M5's calibration actions.
const M5_ACTIONS: [&str; 4] = [
    "cutTriangularCalibrationPattern",
    "acceptTriangularCalibrationResults",
    "cutHoleyCalibrationPattern",
    "acceptHoleyCalibrationResults",
];
const SENDING: &str = "Cannot issue command while sending gcode.";

#[test]
fn every_message_so_far_is_produced_and_fits_its_closed_schema() {
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
    let gcode = Gcode::parse("G21\nG0 X1 Y1\nG1 Z-1\n", &Settings::defaults()).unwrap();
    let load = Request::LoadGcode { gcode: Box::new(gcode), file: None, origin: Origin::Edit };
    effects.extend(rig.send(Event::Request { id: 1, request: load }));
    effects.extend(rig.send(Event::CheckForBoardUpdate));
    effects.extend(rig.send(Event::DirectoryCreated("jobs".into())));
    effects.iter().for_each(|effect| {
        if let Effect::Push(push) = effect {
            contract.validate_frame(&push.frame().encode()).unwrap();
        }
    });
    let seen = contract.seen_commands();
    let missing: Vec<&str> = MESSAGES.into_iter().filter(|c| !seen.contains(*c)).collect();
    assert!(missing.is_empty(), "never produced: {missing:?}");
}

#[test]
fn m2_produces_the_pid_results_message_within_its_closed_schema() {
    let contract = Contract::load();
    let mut rig = Rig::connected();
    let pid = json!({"vMotor": "L", "vStart": "0", "vStop": "1", "vSteps": "1", "vVersion": "2", "KpV": "5",
        "KiV": "0", "KdV": "0.28"});
    let mut effects = rig.act_with("executeVelocityPIDTest", pid, json!(""));
    ["--PID Velocity Test Start--", "12.0,11.5,30", "--PID Velocity Test Stop--"]
        .iter()
        .for_each(|line| effects.extend(rig.rx(line)));
    effects.iter().for_each(|effect| {
        if let Effect::Push(push) = effect {
            contract.validate_frame(&push.frame().encode()).unwrap();
        }
    });
    assert!(contract.seen_commands().contains("updatePIDData"));
}

#[test]
fn actions_so_far_are_refused_while_streaming_exactly_as_their_gates_say() {
    let actions = Contract::shared().actions();
    ACTIONS.iter().chain(&M2_ACTIONS).chain(&M5_ACTIONS).for_each(|command| {
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
        "updateSetting" => json!({"data": {"setting": "toMM", "value": "25.4"}}),
        _ => json!({"data": {}}),
    }
}

const HANDLED_EVENTS: [&str; 5] =
    ["action", "settingRequest", "updateSetting", "checkForGCodeUpdate", "checkForBoardUpdate"];

#[test]
fn every_client_event_is_handled() {
    let events = Contract::shared().client_events("/MaslowCNC");
    events.iter().for_each(|event| {
        let frame = Frame { ns: "/MaslowCNC".into(), event: event.clone(), data: sample(event) };
        assert!(ClientEvent::from_frame(&frame).is_some(), "{event} does not decode");
    });
    let mut sorted = events.clone();
    sorted.sort();
    let mut handled = HANDLED_EVENTS.map(String::from).to_vec();
    handled.sort();
    assert_eq!(sorted, handled);
}
