//! The contract's per-state gate check (plan §3.2): every action in every run state, each state reached by driving
//! the machine against `wc-sim` (a real M0, tool change, alarm and manual-Z prompt), never by setting fields.

mod rig;

use std::collections::BTreeSet;

use rig::sim::SimRig;
use rig::{alerts, settings};
use serde_json::json;
use wc_config::Settings;
use wc_machine::files::{GCODE_LOCKED, Reply, Request, SEND_LOCKED, SETTINGS_LOCKED};
use wc_machine::{Effect, Event, Request as Edit, gate};
use wc_testkit::contract::Contract;

const REFUSALS: [&str; 2] = ["Cannot issue command while", GCODE_LOCKED];

fn refused_in(gate: &str) -> BTreeSet<&'static str> {
    let states: &[&str] = match gate {
        "always" => &[],
        "idleOrPaused" => &["streaming"],
        "idleOrControllerPaused" => &["streaming", "paused"],
        "idle" => &["streaming", "paused", "controllerPaused"],
        "noJob" => &["streaming", "paused", "controllerPaused", "toolChange"],
        other => panic!("unknown gate {other}"),
    };
    states.iter().copied().collect()
}

fn state(rig: &SimRig) -> (i32, bool, bool) {
    let snapshot = rig.machine.snapshot();
    (snapshot.upload_flag, snapshot.job_in_progress, snapshot.manual_z_axis_adjust)
}

fn started(file: &str, rig: SimRig) -> SimRig {
    let mut rig = rig;
    rig.machine.load_gcode(file);
    rig.act("startRun");
    rig.run(500);
    rig
}

fn streaming() -> SimRig {
    let mut rig = SimRig::new();
    rig.sim.move_ms = 60_000;
    let rig = started("G21\nG90\nG0 X10\nG0 X20\nG0 X30\n", rig);
    assert_eq!(state(&rig), (1, true, false));
    rig
}

/// Each state by name, with every way the contract says to reach it.
fn states() -> Vec<(&'static str, &'static str, SimRig)> {
    let mut paused = streaming();
    paused.act("pauseRun");
    assert_eq!(state(&paused), (2, true, false));
    let program_pause = started("G21\nG0 X1\nM0\nG0 X2\n", SimRig::new());
    assert_eq!(state(&program_pause), (0, true, false));
    let mut alarm = streaming();
    let lines = alarm.sim.alarm("Position Lost");
    alarm.deliver(lines);
    alarm.run(500);
    assert_eq!(state(&alarm), (0, true, false));
    let no_z_motor = SimRig::with(settings(&[("Maslow Settings", "zAxis", json!(0))]));
    let manual_z = started("G21\nG0 X1\nG1 Z-3\nG0 X2\n", no_z_motor);
    assert_eq!(state(&manual_z), (0, true, true));
    let mut tool_change = SimRig::new();
    tool_change.send(Event::Request { id: 1, request: Request::SendGcode("T2 M6".into()) });
    tool_change.run(500);
    assert_eq!(state(&tool_change), (0, false, true));
    let idle = SimRig::new();
    assert_eq!(state(&idle), (0, false, false));
    vec![
        ("streaming", "streaming", streaming()),
        ("paused", "Pause", paused),
        ("controllerPaused", "M0", program_pause),
        ("controllerPaused", "ALARM", alarm),
        ("controllerPaused", "Message: adjust Z", manual_z),
        ("toolChange", "T2 M6 outside a job", tool_change),
        ("idle", "idle", idle),
    ]
}

#[test]
fn each_action_is_refused_in_exactly_the_run_states_its_gate_lists() {
    let states = states();
    let failures: Vec<String> = Contract::shared()
        .actions()
        .iter()
        .flat_map(|(command, spec)| {
            let gate = spec["allowedDuring"].as_str().unwrap_or_default();
            let expected = refused_in(gate);
            states.iter().filter_map(move |(name, how, rig)| {
                let mut rig = rig.clone();
                rig.act(command);
                let alerts = alerts(&rig.take().0);
                let refused = alerts.iter().any(|alert| REFUSALS.iter().any(|text| alert.contains(text)));
                (refused != expected.contains(name))
                    .then(|| format!("{command} ({gate}) in {name} via {how}: refused {refused}, alerts {alerts:?}"))
            })
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_machines_gate_table_is_the_contracts() {
    Contract::shared().actions().iter().for_each(|(command, spec)| {
        assert_eq!(gate(command).name(), spec["allowedDuring"], "{command}");
    });
    assert_eq!(gate("noSuchCommand").name(), "idle", "an unknown command falls through to the idle gate");
}

/// Decision 2: every way to change the settings or send lines outside the job, each as the browser sends it.
fn writes() -> Vec<(&'static str, Event, &'static str)> {
    let quick = json!({"kinematicsType": "Linkage", "rotationRadius": 273, "chainOverSprocket": "Top",
        "motorSpacingX": 2938.05, "motorOffsetY": 470});
    let section = |values| Edit::UpdateSettings { section: "maslow".into(), body: json!({ "values": values }) };
    let edit = |request| Event::Edit { id: 9, request };
    let ask = |request| Event::Request { id: 9, request };
    let imported = json!({"Maslow Settings": [{"key": "motorSpacingX", "value": "3001"}]});
    vec![
        ("PUT /api/settings", edit(section(json!({"motorSpacingX": "3000"}))), SETTINGS_LOCKED),
        ("COMport", edit(section(json!({"COMport": "/dev/ttyUSB0"}))), SETTINGS_LOCKED),
        ("quick-configure", edit(Edit::QuickConfigure(quick)), SETTINGS_LOCKED),
        ("updateSetting", Event::UpdateSetting { setting: "toInches".into(), value: json!("1") }, SETTINGS_LOCKED),
        ("updateSetting Z", Event::UpdateSetting { setting: "toMMZ".into(), value: json!("1") }, SETTINGS_LOCKED),
        ("/sendGCode", ask(Request::SendGcode("G0 X1".into())), SEND_LOCKED),
        ("/importFile", ask(Request::ImportIni("[Maslow Settings]\nmotorSpacingX = 3001\n".into())), SETTINGS_LOCKED),
        ("/importFileWCJSON", ask(Request::ImportWebControl(imported)), SETTINGS_LOCKED),
        ("/importRestoreWebControl", ask(Request::RestoreSettings(Box::new(Settings::defaults()))), SETTINGS_LOCKED),
    ]
}

/// The refusal a write met: a 409 body, a refused reply, or an alert.
fn refusal(effects: &[Effect]) -> Option<String> {
    let answered = effects.iter().find_map(|effect| match effect {
        Effect::Respond { response, .. } if response.status == 409 => response.body["error"].as_str().map(String::from),
        Effect::Reply { reply: Reply::Refused(error), .. } => Some(error.clone()),
        _ => None,
    });
    answered.or_else(|| alerts(effects).into_iter().find(|alert| alert.contains("Stop the job first")))
}

#[test]
fn settings_writes_sends_and_imports_are_refused_whenever_the_gcode_is_locked() {
    let failures: Vec<String> = states()
        .into_iter()
        .flat_map(|(name, how, rig)| {
            writes().into_iter().filter_map(move |(write, event, message)| {
                let mut rig = rig.clone();
                let settings = rig.machine.snapshot().settings.clone();
                rig.take();
                rig.send(event);
                let (effects, written) = rig.take();
                let refused = refusal(&effects);
                let expected = (name != "idle").then(|| message.to_string());
                let untouched = name == "idle" || (*rig.machine.snapshot().settings == *settings && written.is_empty());
                (refused != expected || !untouched)
                    .then(|| format!("{write} in {name} via {how}: refused {refused:?}, wrote {written:?}"))
            })
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
