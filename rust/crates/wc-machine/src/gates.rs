//! The run-state gates of `contract/index.json` (`actions.<command>.allowedDuring`), refused with Python's texts.

use crate::job::Flow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    Always,
    IdleOrPaused,
    IdleOrControllerPaused,
    Idle,
    NoJob,
}

impl Gate {
    pub fn name(self) -> &'static str {
        match self {
            Gate::Always => "always",
            Gate::IdleOrPaused => "idleOrPaused",
            Gate::IdleOrControllerPaused => "idleOrControllerPaused",
            Gate::Idle => "idle",
            Gate::NoJob => "noJob",
        }
    }
}

pub const ALWAYS: [&str; 10] = [
    "createDirectory",
    "stopZ",
    "stopRun",
    "pauseRun",
    "resumeRun",
    "toggleCamera",
    "statusRequest",
    "queryCamera",
    "shutdown",
    "TurnOffRPI",
];
pub const IDLE_OR_PAUSED: [&str; 19] = [
    "move",
    "moveTo",
    "moveZ",
    "setMaxZ",
    "setMinZ",
    "clearZ",
    "getZlimits",
    "reportSettings",
    "home",
    "defineZ0",
    "touchZ",
    "moveToDefault",
    "returnToCenter",
    "moveGcodeZ",
    "moveGcodeGoto",
    "moveGcodeIndex",
    "macro1",
    "macro2",
    "clearLogs",
];
pub const NO_JOB: [&str; 6] =
    ["clearGCode", "defineHome", "resetHomeToCenter", "executeVelocityPIDTest", "executePositionPIDTest", "definePark"];

pub const SENDING: &str = "Cannot issue command while sending gcode.";
pub const PAUSED: &str =
    "Cannot issue command while paused sending gcode. You must press STOP before performing this action.";
pub const TOOL_CHANGE_WAITS: &str =
    "Cannot issue command while a tool change waits. Press Resume or STOP before setting home or running a PID test.";

/// Everything else, unknown commands included, waits for no job at all, as Python's dispatch falls through.
pub fn gate(command: &str) -> Gate {
    match command {
        _ if ALWAYS.contains(&command) => Gate::Always,
        _ if IDLE_OR_PAUSED.contains(&command) => Gate::IdleOrPaused,
        "startRun" => Gate::IdleOrControllerPaused,
        _ if NO_JOB.contains(&command) => Gate::NoJob,
        _ => Gate::Idle,
    }
}

/// Why `command` cannot run now, if it cannot. `clearGCode` answers for itself with the gcode routes' text.
pub fn refusal(command: &str, flow: Flow, manual_z: bool) -> Option<&'static str> {
    match gate(command) {
        Gate::Always => None,
        _ if flow == Flow::Streaming => Some(SENDING),
        Gate::IdleOrPaused => None,
        Gate::IdleOrControllerPaused => matches!(flow, Flow::Paused | Flow::Draining).then_some(PAUSED),
        _ if command == "clearGCode" => None,
        _ if flow.in_job() => Some(PAUSED),
        Gate::NoJob if manual_z => Some(TOOL_CHANGE_WAITS),
        Gate::NoJob | Gate::Idle => None,
    }
}
