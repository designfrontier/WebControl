//! The contract's per-state gate check (plan §3.2): every action in every run state, each state reached by driving
//! the machine against `wc-sim` (a real M0, tool change, alarm and manual-Z prompt), never by setting fields.

mod rig;

use std::collections::BTreeSet;

use rig::sim::SimRig;
use rig::{alerts, settings};
use serde_json::json;
use wc_machine::files::{GCODE_LOCKED, Request};
use wc_machine::{Event, gate};
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
