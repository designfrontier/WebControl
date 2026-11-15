//! Settings writes: the settings screens' save, quick configure, the PID test request and the front page's unit
//! switches. Each goes through `Config.setValue`'s rules, so the controller gets the `$` lines Python sends.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use wc_config::python_float_str;
use wc_protocol::machine::{QuickConfigureForm, SettingsUpdate};
use wc_protocol::main::{HomePositionMessage, RequestedSetting};
use wc_protocol::system::PidTest;

use crate::calibration::{Calculated, Kind};
use crate::commands::round4;
use crate::forms;
use crate::machine::{Effect, Link, Machine, RunState, alert, broadcast};

const COMPUTED: &str = "Computed Settings";
const ADVANCED: &str = "Advanced Settings";
const MASLOW: &str = "Maslow Settings";
const MM_PER_INCH: f64 = 25.4;
const SENDING: &str = "Cannot change units while sending gcode.";

/// An HTTP write the machine answers.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    /// `PUT /api/settings/{sectionId}`.
    UpdateSettings { section: String, body: Value },
    /// `POST /api/machine/quick-configure`.
    QuickConfigure(Value),
    /// `POST /api/system/pid-test`.
    PidTest(Value),
    /// `POST /api/calibration/{kind}`, calculated by `calibration::calculate`.
    Calibrated(Calculated),
    /// `POST /api/calibration/{kind}/accept`.
    AcceptCalibration(Kind),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    pub status: u16,
    pub body: Value,
}

impl Response {
    fn ok<T: serde::Serialize>(status: u16, body: &T) -> Response {
        Response { status, body: serde_json::to_value(body).expect("protocol types always serialise") }
    }

    pub(crate) fn error(status: u16, message: &str) -> Response {
        Response { status, body: json!({ "error": message }) }
    }
}

/// The old quick-configure form posted whole numbers without a trailing `.0`.
fn form_text(number: f64) -> Value {
    Value::from(if number.fract() == 0.0 { format!("{number:.0}") } else { python_float_str(number) })
}

impl Machine {
    /// `Config.setValue`: stores the value and queues the `$` lines it and the derived settings send.
    pub(crate) fn write_setting(&mut self, section: &str, key: &str, value: &Value) -> bool {
        let firmware = self.firmware_version;
        let settings = Arc::make_mut(&mut self.settings);
        let lines = wc_firmware::settings::write(settings, firmware, section, key, value);
        lines.map(|lines| self.queue(lines)).is_some()
    }

    pub(crate) fn request(&mut self, request: Request, now: Duration, effects: &mut Vec<Effect>) -> Response {
        match request {
            Request::UpdateSettings { section, body } => self.update_section(&section, &body, now, effects),
            Request::QuickConfigure(body) => self.quick_configure(&body),
            Request::PidTest(body) => self.pid_test(&body, now, effects),
            Request::Calibrated(calculated) => self.calibrated(calculated),
            Request::AcceptCalibration(kind) => self.accept_calibration(kind),
        }
    }

    fn section_now(&self, id: &str) -> Option<wc_protocol::settings::SettingsSection> {
        forms::section(&self.settings, id, self.firmware_version, &self.ports)
    }

    /// `PUT /api/settings/{id}`: all or nothing. Settings not posted are left as stored, where Python rewrote them
    /// as the form's text.
    fn update_section(&mut self, id: &str, body: &Value, now: Duration, effects: &mut Vec<Effect>) -> Response {
        let (Some(title), Some(section)) = (forms::section_title(id), self.section_now(id)) else {
            return Response::error(404, "Not found");
        };
        let update = match SettingsUpdate::parse(body) {
            Ok(update) => update,
            Err(message) => return Response::error(400, &message),
        };
        let errors = forms::errors(&section, &update.values, &self.ports);
        if !errors.is_empty() {
            return Response { status: 400, body: json!({ "errors": errors }) };
        }
        let port = self.settings.value(MASLOW, "COMport").cloned();
        section.settings.iter().for_each(|field| {
            if let Some(value) = update.values.get(&field.key) {
                self.write_setting(title, &field.key, &forms::form_value(field, value));
            }
        });
        if title == MASLOW && self.settings.value(MASLOW, "COMport").cloned() != port {
            self.reopen_port(now, effects);
        }
        Response::ok(200, &self.section_now(id))
    }

    /// The port setting changed while connected: close, and the next tick opens the new one.
    fn reopen_port(&mut self, now: Duration, effects: &mut Vec<Effect>) {
        if self.fixed_port.is_some() || !matches!(self.link, Link::Connected { .. }) {
            return;
        }
        if self.run == RunState::Running {
            self.end_job();
        }
        self.probing = false;
        self.link = Link::Disconnected { retry_at: now };
        effects.push(Effect::ClosePort);
        effects.push(broadcast(&self.connection_status()));
    }

    /// `POST /api/machine/quick-configure` through `Config.updateQuickConfigure`.
    fn quick_configure(&mut self, body: &Value) -> Response {
        let form = match QuickConfigureForm::parse(body) {
            Ok(form) => form,
            Err(message) => return Response::error(400, &message),
        };
        let kinematics = if form.kinematics_type == "Quadrilateral" { "Quadrilateral" } else { "Triangular" };
        [
            (ADVANCED, "kinematicsType", Value::from(kinematics)),
            (ADVANCED, "rotationRadius", form_text(form.rotation_radius)),
            (ADVANCED, "chainOverSprocket", Value::from(form.chain_over_sprocket)),
            (MASLOW, "motorSpacingX", form_text(form.motor_spacing_x)),
            (MASLOW, "motorOffsetY", form_text(form.motor_offset_y)),
        ]
        .iter()
        .for_each(|(section, key, value)| {
            self.write_setting(section, key, value);
        });
        Response::ok(200, &forms::quick_configure(&self.settings))
    }

    /// `POST /api/system/pid-test`: the action the old PID screen sent, refused while a job holds the gcode.
    fn pid_test(&mut self, body: &Value, now: Duration, effects: &mut Vec<Effect>) -> Response {
        let test = match PidTest::parse(body) {
            Ok(test) => test,
            Err(message) => return Response::error(400, &message),
        };
        if self.run != RunState::Idle {
            return Response::error(409, "Stop the job before starting a PID test.");
        }
        self.action(&crate::pid::test_action(&test), now, effects);
        Response { status: 202, body: json!({"status": "sent"}) }
    }

    /// `Actions.updateSetting`: the front page's unit switches; anything else is ignored, as in Python.
    pub(crate) fn update_setting(&mut self, setting: &str, value: &Value, effects: &mut Vec<Effect>) {
        let units = matches!(setting, "toInches" | "toMM" | "toInchesZ" | "toMMZ");
        if units && self.run == RunState::Running {
            return effects.push(alert(SENDING));
        }
        let done = match setting {
            "toInches" | "toMM" => self.switch_units(setting == "toInches", value, effects),
            "toInchesZ" | "toMMZ" => {
                let units_z = if setting == "toInchesZ" { "INCHES" } else { "MM" };
                self.write_setting(COMPUTED, "unitsZ", &Value::from(units_z));
                self.write_setting(COMPUTED, "distToMoveZ", value);
                ["unitsZ", "distToMoveZ"].iter().for_each(|key| self.push_setting(key, effects));
                true
            }
            _ => true,
        };
        if !done {
            effects.push(alert("Error updating setting"));
        }
    }

    fn push_setting(&self, key: &str, effects: &mut Vec<Effect>) {
        let value = self.settings.value(COMPUTED, key).cloned().unwrap_or(Value::Null);
        effects.push(broadcast(&RequestedSetting { setting: key.into(), value }));
    }

    /// Units, tolerance, the jog step as the UI converted it, and home scaled to 4 places.
    fn switch_units(&mut self, inches: bool, step: &Value, effects: &mut Vec<Effect>) -> bool {
        let (units, scale, tolerance, line) = match inches {
            true => ("INCHES", 1.0 / MM_PER_INCH, 0.020, "G20 "),
            false => ("MM", MM_PER_INCH, 0.5, "G21 "),
        };
        self.write_setting(COMPUTED, "units", &Value::from(units));
        self.write_setting(COMPUTED, "tolerance", &Value::from(tolerance));
        self.queue([line.to_string()]);
        self.write_setting(COMPUTED, "distToMove", step);
        let home = |key: &str| self.settings.value(ADVANCED, key).and_then(wc_config::python_float);
        let Some((x, y)) = home("homeX").zip(home("homeY")) else {
            return false;
        };
        let (x, y) = (round4(x * scale), round4(y * scale));
        self.write_setting(ADVANCED, "homeX", &Value::from(x));
        self.write_setting(ADVANCED, "homeY", &Value::from(y));
        ["units", "distToMove"].iter().for_each(|key| self.push_setting(key, effects));
        effects.push(broadcast(&HomePositionMessage { xval: x, yval: y }));
        true
    }
}
