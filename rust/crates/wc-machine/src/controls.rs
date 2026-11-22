//! What the menus and the control screen's buttons show: the four macros, pushed to every client when they change.

use wc_config::Settings;
use wc_protocol::main::{Macro, MacrosMessage};

use crate::machine::{Effect, Machine, broadcast};

pub const MACROS: [&str; 4] = ["macro1", "macro2", "macro3", "macro4"];
const SECTION: &str = "Maslow Settings";

/// Each macro's title and whether its gcode is blank.
pub fn macros(settings: &Settings) -> Vec<Macro> {
    (1..=4)
        .map(|number| {
            let text = |suffix: &str| settings.text_or_empty(SECTION, &format!("macro{number}{suffix}"));
            Macro::new(number, &text("_title"), text("").trim().is_empty())
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Shown {
    macros: Vec<Macro>,
}

impl Shown {
    pub fn new(settings: &Settings) -> Shown {
        Shown { macros: macros(settings) }
    }
}

impl Machine {
    /// After each event: every client hears a changed macro.
    pub(crate) fn publish_controls(&mut self, effects: &mut Vec<Effect>) {
        if std::sync::Arc::ptr_eq(&self.settings, &self.saved) {
            return;
        }
        let macros = macros(&self.settings);
        if macros != self.controls.macros {
            effects.push(broadcast(&MacrosMessage { macros: macros.clone() }));
            self.controls.macros = macros;
        }
    }
}
