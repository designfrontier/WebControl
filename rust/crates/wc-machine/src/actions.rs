//! `Actions.processAction` for the link's commands: the gates, then what each command queues or does.

use std::time::Duration;

use serde_json::Value;
use wc_config::python_float_str;
use wc_firmware::settings::setting_line;
use wc_protocol::main::{HomePositionMessage, PauseButton, RequestedSetting, Units};
use wc_protocol::{Action, float};

use crate::clearance::Raise;
use crate::commands;
use crate::gates;
use crate::job::Flow;
use crate::machine::{Effect, Link, Machine, alert, broadcast};

pub(crate) const PROBING: &str = "Wait for Touch Z to finish or press Stop";
const NOT_CONNECTED: &str = "The controller is not connected.";
const Z_LIMITS_UNSUPPORTED: &str = "Z-axis limits need holey firmware 51.292 or later.";
const JOB_LINES_HELD: &str = "The controller paused with lines of the job still in its buffer, which would run before \
                              this move. Press Resume or STOP first.";
const ADJUST_Z_FIRST: &str = "The controller is waiting for Z to be adjusted by hand. Adjust it, then press Resume.";
const GCODE_LOADING: &str = "Gcode is still loading. Start the job when the preview appears.";
const CLEARED_Z_LIMIT_MM: i32 = 9999;
/// Stop, Pause and the actions that send the controller nothing.
const SAFE_WHILE_PROBING: [&str; 13] = [
    "stopRun",
    "stopZ",
    "pauseRun",
    "createDirectory",
    "toggleCamera",
    "statusRequest",
    "queryCamera",
    "shutdown",
    "TurnOffRPI",
    "updatePorts",
    "clearLogs",
    "boardProcessGCode",
    "boardClearBoard",
];
/// Commands that reach the controller, so they need it connected.
const MACHINE_COMMANDS: [&str; 17] = [
    "move",
    "moveTo",
    "moveZ",
    "reportSettings",
    "home",
    "defineZ0",
    "touchZ",
    "moveToDefault",
    "returnToCenter",
    "macro1",
    "macro2",
    "setMaxZ",
    "setMinZ",
    "clearZ",
    "getZlimits",
    "cutTriangularCalibrationPattern",
    "cutHoleyCalibrationPattern",
];
/// Python's `MACHINE_COMMANDS`: the manual commands a firmware pause refuses or a jog's `~` releases.
const MANUAL_COMMANDS: usize = 15;

fn is_blank(value: &Value) -> bool {
    value.as_str() == Some("")
}

impl Machine {
    pub(crate) fn action(&mut self, action: &Action, now: Duration, effects: &mut Vec<Effect>) {
        let command = action.command.as_str();
        if self.upgrade.holds_link() && !crate::upgrade::allowed_while_flashing(command) {
            return effects.push(alert("Wait for the firmware upgrade to finish."));
        }
        let optical_allowed = SAFE_WHILE_PROBING.contains(&command) || crate::optical::COMMANDS.contains(&command);
        if self.optical_holds() && !optical_allowed {
            return effects.push(alert(crate::optical::RUNNING));
        }
        if self.probing && !SAFE_WHILE_PROBING.contains(&command) {
            return effects.push(alert(PROBING));
        }
        if let Some(refusal) = self.restart_refusal().filter(|_| !SAFE_WHILE_PROBING.contains(&command)) {
            return effects.push(alert(refusal));
        }
        let machine_command = MACHINE_COMMANDS.contains(&command);
        let manual_command = MACHINE_COMMANDS[..MANUAL_COMMANDS].contains(&command);
        if manual_command && self.job.firmware_paused && self.job.flow != Flow::Streaming {
            if self.job.holds_job_lines {
                return effects.push(alert(JOB_LINES_HELD));
            }
            // A `~` here would tell the firmware Z was adjusted by hand ($16=0) and carry on with the line.
            if self.job.manual_z == Some(false) {
                return effects.push(alert(ADJUST_Z_FIRST));
            }
        }
        if let Some(refusal) = gates::refusal(command, self.job.flow, self.job.manual_z.is_some()) {
            return effects.push(alert(refusal));
        }
        if let Some(refusal) = self.alarm_refusal(command) {
            return effects.push(alert(refusal));
        }
        if command == "startRun" && self.loads > 0 {
            return effects.push(alert(GCODE_LOADING));
        }
        if machine_command && !matches!(self.link, Link::Connected { .. }) {
            return effects.push(alert(NOT_CONNECTED));
        }
        if manual_command && self.job.firmware_paused {
            // Streaming stays held, so releasing the firmware lets the jog run now instead of on Resume.
            self.release_firmware(now, effects);
        }
        let failure = match command {
            "stopRun" => self.stop_run(now, effects),
            "stopZ" => self.stop_z(now, effects),
            // statusRequest only asks for the camera's status, and the camera is out of scope.
            "statusRequest" => None,
            "shutdown" => {
                effects.push(Effect::Exit);
                None
            }
            "TurnOffRPI" => {
                effects.push(Effect::PowerOff);
                None
            }
            "move" => self.jog(action),
            "moveTo" => self.move_to(action),
            "moveZ" => self.move_z(action),
            "home" => self.home(),
            "returnToCenter" => self.return_to_center(),
            "moveToDefault" => {
                let chain = self.settings.text_or_empty("Advanced Settings", "chainExtendLength");
                self.queue(commands::move_to_default(&chain));
                None
            }
            "defineZ0" => {
                self.queue(["G10 Z0 ".to_string()]);
                None
            }
            "touchZ" => self.touch_z(),
            "reportSettings" => {
                self.queue(["$$".to_string()]);
                None
            }
            "macro1" | "macro2" => self.run_macro(command),
            "setMaxZ" => self.set_z_limit(63),
            "setMinZ" => self.set_z_limit(64),
            "clearZ" => self.z_limits(|| {
                [setting_line(63, &CLEARED_Z_LIMIT_MM.into()), setting_line(64, &(-CLEARED_Z_LIMIT_MM).into())].into()
            }),
            "getZlimits" => self.z_limits(|| vec!["B20".into()]),
            "defineHome" => self.define_home(&action.arg, &action.arg1, effects).or_else(|| {
                self.push_units(effects);
                self.push_gcode(effects);
                None
            }),
            "resetHomeToCenter" => self.define_home(&Value::from(0), &Value::from(0), effects).or_else(|| {
                self.push_gcode(effects);
                None
            }),
            "clearGCode" => self.clear_gcode(effects),
            "createDirectory" => {
                effects.push(Effect::CreateDirectory(action.arg_text()));
                None
            }
            "boardProcessGCode" | "boardClearBoard" => {
                self.board_action(command, effects);
                None
            }
            "moveGcodeZ" | "moveGcodeGoto" | "moveGcodeIndex" => self.move_gcode(command, &action.arg, effects),
            "updatePorts" => {
                effects.push(Effect::ListPorts);
                None
            }
            "clearLogs" => {
                effects.push(Effect::ClearLogs);
                None
            }
            "startRun" => self.start_run(now, effects),
            "pauseRun" => self.pause_run(effects),
            "resumeRun" => self.resume_run(now, effects),
            crate::upgrade::COMMAND => self.upgrade_action(action, now, effects),
            "upgradeStockFirmware" | "upgradeCustomFirmware" => Some(self.other_firmware(command)),
            _ if crate::setup::COMMANDS.contains(&command) => self.setup_action(action, effects),
            _ if crate::pid::COMMANDS.contains(&command) => self.pid_action(action),
            _ if crate::calibration::COMMANDS.contains(&command) => self.calibration_action(command),
            _ if crate::optical::COMMANDS.contains(&command) => self.optical_action(command, now, effects),
            _ => Some(format!("Function not currently implemented.. Sorry.[{command}]")),
        };
        if let Some(text) = failure {
            effects.push(alert(&text));
        }
        if matches!(command, "stopRun" | "stopZ") {
            self.optical_stopped(None, effects);
        }
    }

    /// Python left a running job streaming after stopZ's `!` flushed its lines; here it stops the job too.
    fn stop_z(&mut self, now: Duration, effects: &mut Vec<Effect>) -> Option<String> {
        let held = self.job.flow.in_job() || self.job.manual_z.is_some();
        self.stopped(now, effects);
        self.clear_alarm(effects);
        if held {
            effects.push(crate::job::pause_button(PauseButton::Pause));
        }
        None
    }

    fn jog(&mut self, action: &Action) -> Option<String> {
        let distance = float(&action.arg1)?;
        let diagonal =
            matches!(self.settings.value("WebControl Settings", "diagonalMove"), Some(v) if v == 1 || v == true);
        let Some(lines) = commands::jog(&action.arg_text(), distance, diagonal) else {
            return Some("Error with initiating move.".into());
        };
        self.set_setting("Computed Settings", "distToMove", Value::from(distance));
        self.queue(lines);
        None
    }

    /// `moveTo` needs a JSON number for `arg`; text fails Python's bed-bounds check (wart 13).
    fn move_to(&mut self, action: &Action) -> Option<String> {
        let refused = Some("Error with initiating move.".to_string());
        let (Some(x), Some(y)) = (action.arg.as_f64(), float(&action.arg1)) else {
            return refused;
        };
        let bed = |key: &str| self.settings.number("Maslow Settings", key);
        match commands::move_to(x, y, self.units(), bed("bedWidth"), bed("bedHeight")) {
            Some(line) => {
                self.queue([line]);
                None
            }
            None => refused,
        }
    }

    fn move_z(&mut self, action: &Action) -> Option<String> {
        let distance = float(&action.arg1)?;
        self.set_setting("Computed Settings", "distToMoveZ", Value::from(distance));
        let units_z = Units::from_setting(&self.settings.text_or_empty("Computed Settings", "unitsZ"));
        self.queue(commands::move_z(&action.arg_text(), distance, self.units(), units_z));
        None
    }

    fn safe_height(&self) -> Option<f64> {
        float(self.settings.value("Maslow Settings", "zAxisSafeHeight")?)
    }

    fn home(&mut self) -> Option<String> {
        let Some(safe) = self.safe_height() else {
            return Some("Error with returning to home.".into());
        };
        let text = |key: &str| self.settings.text("Advanced Settings", key).unwrap_or_default();
        let lines = commands::home(&text("homeX"), &text("homeY"));
        self.queue_raised(safe, lines);
        None
    }

    fn queue_raised(&mut self, safe_height_mm: f64, travel: Vec<String>) {
        self.queue([commands::BEFORE_RAISE.to_string()]);
        self.queue_raise(Raise::Manual { units: self.units(), safe_height_mm });
        self.queue(travel);
    }

    fn return_to_center(&mut self) -> Option<String> {
        let Some(safe) = self.safe_height() else {
            return Some("Error with returning to center".into());
        };
        self.queue_raised(safe, commands::return_to_center());
        None
    }

    fn touch_z(&mut self) -> Option<String> {
        let Some(plunge) = self.settings.value("Advanced Settings", "maxTouchProbePlungeDistance").and_then(float)
        else {
            return Some("Error with touching.".into());
        };
        self.queue([commands::touch_z(plunge, self.units())]);
        self.probing = true;
        None
    }

    /// B12: only 51.292 and later store Z limits; elsewhere B17-B19 do nothing or corrupt `$43`/`$44`.
    fn z_limits(&mut self, lines: impl FnOnce() -> Vec<String>) -> Option<String> {
        let build = wc_firmware::boot::firmware_build(self.firmware_version, self.rewrite_notice);
        if !wc_firmware::boot::supports_z_limits(build.as_deref()) {
            let reported = build.unwrap_or_else(|| "no version".into());
            return Some(format!("{Z_LIMITS_UNSUPPORTED} The controller reports {reported}."));
        }
        self.queue(lines());
        None
    }

    /// The firmware keeps Z limits in mm, whatever the machine's units.
    fn set_z_limit(&mut self, key: u32) -> Option<String> {
        let z_mm = if self.units() == Units::Inches { self.position.z * 25.4 } else { self.position.z };
        self.z_limits(|| vec![setting_line(key, &((z_mm * 1000.0).round() / 1000.0).into())])
    }

    fn run_macro(&mut self, command: &str) -> Option<String> {
        let text = self.settings.text_or_empty("Maslow Settings", command);
        self.queue([text]);
        None
    }

    /// Both arguments blank: the sled's position from the last status report; else inches, stored in machine units.
    fn define_home(&mut self, x: &Value, y: &Value, effects: &mut Vec<Effect>) -> Option<String> {
        let (home_x, home_y) = match (is_blank(x) || is_blank(y), float(x), float(y)) {
            (true, _, _) => (commands::round4(self.position.x), commands::round4(self.position.y)),
            (false, Some(x), Some(y)) => commands::home_from(x, y, self.units()),
            _ => return Some("Error with defining home.".into()),
        };
        self.set_setting("Advanced Settings", "homeX", Value::from(python_float_str(home_x)));
        self.set_setting("Advanced Settings", "homeY", Value::from(python_float_str(home_y)));
        effects.push(broadcast(&HomePositionMessage { xval: home_x, yval: home_y }));
        None
    }

    /// `unitsUpdate`: a gcode file can change the units, so the UI hears them again.
    pub(crate) fn push_units(&self, effects: &mut Vec<Effect>) {
        let units = self.settings.value("Computed Settings", "units").cloned().unwrap_or(Value::Null);
        effects.push(broadcast(&RequestedSetting { setting: "units".into(), value: units }));
    }
}
