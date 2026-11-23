//! What the menus and the control screen's buttons show: the four macros and the four button slots, pushed to every
//! client when they change.

use std::sync::Arc;

use serde_json::Value;
use wc_config::Settings;
use wc_protocol::main::{ControlButtons, Macro, MacrosMessage};

use crate::edits::Response;
use crate::machine::{Effect, Machine, broadcast};

pub const MACROS: [&str; 4] = ["macro1", "macro2", "macro3", "macro4"];
const MACRO_SECTION: &str = "Maslow Settings";
pub const BUTTONS_SECTION: &str = "WebControl Settings";
/// Changed only through `PUT /api/display/controls`, so it stays off the section forms.
pub const BUTTONS_KEY: &str = "controlButtons";
const BUTTONS: [&str; 8] = ["park", "zAxis", "setHome", "setPark", "macro1", "macro2", "macro3", "macro4"];
const DEFAULT_BUTTONS: [&str; 4] = ["park", "zAxis", "setHome", "setPark"];
const SLOTS: usize = 4;
const INVALID: &str = "Expected {\"buttons\": [four of park, zAxis, setHome, setPark, macro1 to macro4, none twice]}";

/// Each macro's title and whether its gcode is blank.
pub fn macros(settings: &Settings) -> Vec<Macro> {
    (1..=4)
        .map(|number| {
            let text = |suffix: &str| settings.text_or_empty(MACRO_SECTION, &format!("macro{number}{suffix}"));
            Macro::new(number, &text("_title"), text("").trim().is_empty())
        })
        .collect()
}

/// Four known buttons, none twice.
fn valid(buttons: &[&str]) -> bool {
    let unique = buttons.iter().enumerate().all(|(at, button)| !buttons[..at].contains(button));
    buttons.len() == SLOTS && unique && buttons.iter().all(|button| BUTTONS.contains(button))
}

/// The stored slots; a hand-edited value that is not four known buttons, none twice, reads as the defaults.
pub fn buttons(settings: &Settings) -> Vec<String> {
    let stored = settings.text_or_empty(BUTTONS_SECTION, BUTTONS_KEY);
    let listed: Vec<&str> = stored.split(',').map(str::trim).collect();
    let chosen = if valid(&listed) { listed } else { DEFAULT_BUTTONS.to_vec() };
    chosen.into_iter().map(String::from).collect()
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Shown {
    macros: Vec<Macro>,
    buttons: Vec<String>,
}

impl Shown {
    pub fn new(settings: &Settings) -> Shown {
        Shown { macros: macros(settings), buttons: buttons(settings) }
    }
}

impl Machine {
    /// Stores only the slots: no derived settings, no `$` lines, so neither a job nor a scan holds them back.
    pub(crate) fn update_buttons(&mut self, body: &Value) -> Response {
        let requested = body
            .as_object()
            .filter(|fields| fields.len() == 1)
            .and_then(|fields| fields.get("buttons"))
            .and_then(Value::as_array)
            .and_then(|list| list.iter().map(Value::as_str).collect::<Option<Vec<&str>>>())
            .filter(|buttons| valid(buttons));
        let Some(buttons) = requested else {
            return Response::error(400, INVALID);
        };
        let text = buttons.join(",");
        if self.settings.text_or_empty(BUTTONS_SECTION, BUTTONS_KEY) != text {
            Arc::make_mut(&mut self.settings).set_value(BUTTONS_SECTION, BUTTONS_KEY, Value::from(text));
        }
        Response::ok(200, &ControlButtons { buttons: buttons.into_iter().map(String::from).collect() })
    }

    /// After each event that changed the settings: every client hears changed macros or slots.
    pub(crate) fn publish_controls(&mut self, effects: &mut Vec<Effect>) {
        if Arc::ptr_eq(&self.settings, &self.saved) {
            return;
        }
        let macros = macros(&self.settings);
        if macros != self.controls.macros {
            effects.push(broadcast(&MacrosMessage { macros: macros.clone() }));
            self.controls.macros = macros;
        }
        let buttons = buttons(&self.settings);
        if buttons != self.controls.buttons {
            effects.push(broadcast(&ControlButtons { buttons: buttons.clone() }));
            self.controls.buttons = buttons;
        }
    }
}
