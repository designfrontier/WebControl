//! Four macros: Macro 3 and Macro 4 run exactly as Macro 1 and Macro 2 do, and every client hears a changed macro.

mod rig;

use rig::{Rig, alerts, pushed, settings};
use serde_json::json;
use wc_machine::{Effect, Event, Request};

fn with_macros() -> Rig {
    let macros = [1, 2, 3, 4]
        .map(|n| ("Maslow Settings", ["macro1", "macro2", "macro3", "macro4"][n - 1], json!(format!("G0 X{n}"))));
    let mut rig = Rig::connected_with(settings(&macros));
    rig.clear_written();
    rig
}

#[test]
fn each_macro_sends_its_gcode() {
    let mut rig = with_macros();
    ["macro1", "macro2", "macro3", "macro4"].iter().for_each(|command| {
        rig.act(command);
    });
    assert_eq!(rig.run_to_quiet(), ["G0 X1", "G0 X2", "G0 X3", "G0 X4"]);
}

#[test]
fn macro_3_and_4_are_gated_as_macro_1_is() {
    let mut rig = with_macros();
    rig.machine.load_gcode("G21\nG1 X10 F100\nG1 X20\n");
    rig.act("startRun");
    let refusals: Vec<Vec<String>> = ["macro1", "macro3", "macro4"].iter().map(|c| alerts(&rig.act(c))).collect();
    assert!(refusals.iter().all(|alert| *alert == ["Cannot issue command while sending gcode."]), "{refusals:?}");
    rig.act("pauseRun");
    rig.run_to_quiet();
    rig.act("macro4");
    assert_eq!(rig.run_to_quiet(), ["G0 X4"], "allowed while paused, as Macro 1 is");
}

#[test]
fn macro_3_releases_a_tool_change_pause_as_macro_1_does() {
    let released = |command: &str| {
        let mut rig = with_macros();
        rig.send(Event::Request { id: 1, request: wc_machine::files::Request::SendGcode("T2 M6".into()) });
        rig.run_to_quiet();
        rig.rx("Tool Change: Please insert tool 2");
        rig.rx("Maslow Paused");
        rig.act(command);
        rig.run_to_quiet()
    };
    assert_eq!(released("macro1"), ["~", "G0 X1"]);
    assert_eq!(released("macro3"), ["~", "G0 X3"]);
}

#[test]
fn a_changed_macro_is_pushed_with_its_title_and_whether_it_is_empty() {
    let mut rig = Rig::connected();
    let body = json!({"values": {"macro3": "M3", "macro4_title": "Vacuum"}});
    let effects = rig.send(Event::Edit { id: 1, request: Request::UpdateSettings { section: "maslow".into(), body } });
    let macros = pushed(&effects, "macros");
    assert_eq!(macros.len(), 1);
    assert_eq!(macros[0]["macros"][2], json!({"command": "macro3", "title": "Macro 3", "empty": false}));
    assert_eq!(macros[0]["macros"][3], json!({"command": "macro4", "title": "Vacuum", "empty": true}));
    assert!(!rig.advance(100).iter().any(|e| matches!(e, Effect::Push(p) if p.data["command"] == "macros")));
}
