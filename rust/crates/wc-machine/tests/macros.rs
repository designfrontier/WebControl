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

fn put_buttons(rig: &mut Rig, body: serde_json::Value) -> (wc_machine::Response, Vec<Effect>) {
    let effects = rig.send(Event::Edit { id: 4, request: Request::ControlButtons(body) });
    let response = effects.iter().find_map(|effect| match effect {
        Effect::Respond { response, .. } => Some(response.clone()),
        _ => None,
    });
    (response.expect("answered"), effects)
}

#[test]
fn the_control_buttons_are_four_known_buttons_none_twice_and_every_client_hears_them() {
    let mut rig = Rig::connected();
    let buttons = |rig: &Rig| wc_machine::controls::buttons(&rig.machine.snapshot().settings);
    assert_eq!(buttons(&rig), ["park", "zAxis", "setHome", "setPark"]);
    let chosen = json!(["macro4", "zToZero", "zAxis", "zToSafeHeight"]);
    let (response, effects) = put_buttons(&mut rig, json!({ "buttons": chosen }));
    assert_eq!((response.status, response.body.clone()), (200, json!({ "buttons": chosen })));
    assert_eq!(pushed(&effects, "controlButtons"), [json!({ "buttons": chosen })]);
    assert!(effects.iter().any(|effect| matches!(effect, Effect::SaveSettings(_))));
    let (_, effects) = put_buttons(&mut rig, json!({ "buttons": chosen }));
    assert!(pushed(&effects, "controlButtons").is_empty(), "unchanged");

    let refused = [
        json!({"buttons": ["park", "park", "zAxis", "setHome"]}),
        json!({"buttons": ["park", "zAxis", "setHome"]}),
        json!({"buttons": ["park", "zAxis", "setHome", "macro5"]}),
        json!({"buttons": ["park", "zAxis", "setHome", "setPark"], "theme": "dark"}),
        json!(["park", "zAxis", "setHome", "setPark"]),
    ];
    refused.into_iter().for_each(|body| assert_eq!(put_buttons(&mut rig, body).0.status, 400));
    assert_eq!(buttons(&rig), ["macro4", "zToZero", "zAxis", "zToSafeHeight"]);
}

#[test]
fn a_hand_edited_control_buttons_setting_that_is_not_valid_reads_as_the_defaults() {
    let odd = settings(&[("WebControl Settings", "controlButtons", json!("park,park,zAxis,setHome"))]);
    assert_eq!(wc_machine::controls::buttons(&odd), ["park", "zAxis", "setHome", "setPark"]);
    let spaced = settings(&[("WebControl Settings", "controlButtons", json!(" macro1, macro2 ,zAxis,park"))]);
    assert_eq!(wc_machine::controls::buttons(&spaced), ["macro1", "macro2", "zAxis", "park"]);
}

#[test]
fn the_control_buttons_stay_off_the_webcontrol_settings_form() {
    let section = wc_machine::forms::section(&settings(&[]), "webcontrol", 51.27, false, false, &[]).unwrap();
    assert!(!section.settings.iter().any(|field| field.key == "controlButtons"));
}
