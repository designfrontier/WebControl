//! The UI projection (`Background/UIProcessor.py`): position, error, health and status pushes, throttled as
//! Python throttles them, and the answers to `settingRequest`.

use serde_json::Value;
use wc_config::Settings;
use wc_firmware::{PositionError, Status};
use wc_kinematics::{Geometry, Kinematics};
use wc_protocol::main::{
    ErrorValueMessage, HealthMessage, HomePositionMessage, HostAddress, PositionMessage, RequestedSetting,
};

use crate::machine::{Effect, Machine, broadcast};
use crate::work::{NO_COMPUTED_POSITION, Work};

/// No firmware reports motor power; the classic UI shows this value as green.
const MOTOR_POWER: f64 = 15.0;
/// `data.feedRate`, which nothing updates.
const REQUESTED_FEED: f64 = 20.0;
const VELOCITY_FILTER: f64 = 0.5;
/// `initializeSettings`: each model parameter as the firmware key `updateSetting` takes it.
const MODEL_SETTINGS: [(u32, &str, &str); 13] = [
    (2, "Maslow Settings", "motorSpacingX"),
    (13, "Computed Settings", "distPerRot"),
    (1, "Maslow Settings", "bedHeight"),
    (0, "Maslow Settings", "bedWidth"),
    (3, "Maslow Settings", "motorOffsetY"),
    (40, "Advanced Settings", "leftChainTolerance"),
    (41, "Advanced Settings", "rightChainTolerance"),
    (45, "Advanced Settings", "chainElasticity"),
    (46, "Maslow Settings", "sledWeight"),
    (8, "Advanced Settings", "rotationRadius"),
    (37, "Advanced Settings", "chainSagCorrection"),
    (38, "Computed Settings", "chainOverSprocketComputed"),
    (7, "Computed Settings", "kinematicsTypeComputed"),
];

/// The model `data.holeyKinematics` starts from; a setting that is missing or not a number keeps the default.
pub(crate) fn computed_model(settings: &Settings) -> Kinematics {
    MODEL_SETTINGS.iter().fold(Kinematics::new(Geometry::default()), |mut model, (key, section, name)| {
        if let Some(text) = settings.text(section, name) {
            model.update_setting(*key, &text);
        }
        model
    })
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PositionTracker {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub previous: [f64; 3],
    pub time_previous: f64,
    pub velocity_previous: f64,
    pub was_moving: bool,
}

impl Default for PositionTracker {
    fn default() -> Self {
        PositionTracker {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            previous: [-99990.0; 3],
            time_previous: -99990.0,
            velocity_previous: 0.0,
            was_moving: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ErrorTracker {
    pub previous: (f64, f64),
}

impl Default for ErrorTracker {
    fn default() -> Self {
        ErrorTracker { previous: (-99999.0, -99999.0) }
    }
}

impl Machine {
    /// `setPosOnScreen`: sent only when the sled moved, plus once after it stops.
    pub(crate) fn status_report(&mut self, status: Status, now: std::time::Duration, effects: &mut Vec<Effect>) {
        self.pacing_status(&status);
        self.bit.reported(status.z);
        self.sled.reported(status.x, status.y);
        let axes = [("x", status.x), ("y", status.y), ("z", status.z)].map(|(axis, value)| {
            if value.is_nan() {
                effects.push(broadcast(&wc_protocol::main::ControllerMessage(format!(
                    "Unable to resolve {axis} Kinematics."
                ))));
                0.0
            } else {
                value
            }
        });
        let tracker = &mut self.position;
        [tracker.x, tracker.y, tracker.z] = axes;
        let time = now.as_secs_f64();
        let moved: f64 = axes.iter().zip(tracker.previous).map(|(now, before)| (now - before).abs()).sum();
        let distance =
            axes.iter().zip(tracker.previous).map(|(now, before)| (now - before).powi(2)).sum::<f64>().sqrt();
        let mut velocity = (1.0 - VELOCITY_FILTER) * tracker.velocity_previous
            + VELOCITY_FILTER * distance / (time - tracker.time_previous) * 60.0;
        if !(tracker.was_moving || moved > 0.01) {
            return;
        }
        tracker.was_moving = moved > 0.01;
        if !tracker.was_moving {
            velocity = 0.0;
        }
        tracker.previous = axes;
        tracker.velocity_previous = velocity;
        tracker.time_previous = time;
        let progress = 100.0 * self.index as f64 / (self.gcode.len().saturating_sub(1).max(1)) as f64;
        effects.push(broadcast(&PositionMessage {
            xval: axes[0],
            yval: axes[1],
            zval: axes[2],
            vel: if velocity.is_finite() { velocity } else { 0.0 },
            rqvel: REQUESTED_FEED,
            pcom: format!("{:.1}%", progress.abs()),
            state: status.mode.label().into(),
            lpwr: MOTOR_POWER,
            rpwr: MOTOR_POWER,
            zpwr: MOTOR_POWER,
        }));
    }

    /// `setErrorOnScreen`: errors as a fraction of the limit, sent when they change by 0.001 or more. The computed
    /// position is solved off the machine task, and only for a report that goes out.
    pub(crate) fn position_error(&mut self, error: PositionError, effects: &mut Vec<Effect>) {
        self.pacing_position_error(&error);
        let limit = self.settings.number("Advanced Settings", "positionErrorLimit");
        if limit == 0.0 {
            return;
        }
        let enabled = self.settings.number("WebControl Settings", "computedPosition") as i64;
        let enabled = if error.chains.is_some() { enabled } else { 0 };
        let (left, right) = (error.left / limit, error.right / limit);
        self.reported_buffer = error.space;
        let (left_before, right_before) = self.errors.previous;
        if (left - left_before).abs() + (right - right_before).abs() < 0.001 {
            return;
        }
        self.errors.previous = (left, right);
        let report = ErrorValueMessage {
            left_error: left.abs(),
            right_error: right.abs(),
            computed_x: NO_COMPUTED_POSITION,
            computed_y: NO_COMPUTED_POSITION,
            computed_enabled: enabled,
        };
        match error.chains.filter(|_| enabled > 0) {
            Some(chains) => {
                let (model, guess) = (Box::new(self.kinematics.clone()), (self.position.x, self.position.y));
                self.compute(Work::ComputedPosition { model, chains, guess, report }, effects);
            }
            None => effects.push(broadcast(&report)),
        }
    }

    pub(crate) fn health(&mut self, cpu: f64, effects: &mut Vec<Effect>) {
        let buffering = self.settings.number("Maslow Settings", "bufferOn") != 0.0;
        effects.push(broadcast(&HealthMessage {
            cpu_usage: cpu,
            buffer_size: if buffering { self.reported_buffer } else { -1 },
            controller_firmware_version: self.firmware_version,
        }));
        self.last_status = None;
    }

    /// `statusMessage` whenever what it carries changes, and after every health message.
    pub(crate) fn status_if_changed(&mut self, effects: &mut Vec<Effect>) {
        let status = self.snapshot().status();
        if self.last_status.as_ref() != Some(&status) {
            effects.push(broadcast(&status));
            self.last_status = Some(status);
        }
    }

    /// Everything a client keeps from pushes, sent to `sid` alone.
    pub(crate) fn resync(&mut self, sid: &str, effects: &mut Vec<Effect>) {
        let mut pushes = vec![
            broadcast(&HostAddress { host_address: self.host_address.clone() }),
            broadcast(&self.connection_status()),
            broadcast(&self.snapshot().status()),
            broadcast(&RequestedSetting {
                setting: "pauseButtonSetting".into(),
                value: serde_json::to_value(self.job.pause_button()).unwrap_or(Value::Null),
            }),
            broadcast(&HomePositionMessage {
                xval: self.settings.number("Advanced Settings", "homeX"),
                yval: self.settings.number("Advanced Settings", "homeY"),
            }),
        ];
        self.push_gcode_position_to(sid, &mut pushes);
        self.gcode_update_requested(&mut pushes);
        self.push_gcode_name(&mut pushes);
        self.push_board(&mut pushes);
        pushes.push(self.pacing_shown());
        effects.extend(pushes.into_iter().map(|effect| match effect {
            Effect::Push(push) => Effect::Push(push.to_client(sid)),
            other => other,
        }));
    }

    /// `processSettingRequest`; every answer is a broadcast (wart 12).
    pub(crate) fn setting_request(&mut self, section: &str, setting: &str, effects: &mut Vec<Effect>) {
        let answer = match setting {
            "homePosition" => {
                let (x, y) = (
                    self.settings.number("Advanced Settings", "homeX"),
                    self.settings.number("Advanced Settings", "homeY"),
                );
                return effects.push(broadcast(&HomePositionMessage { xval: x, yval: y }));
            }
            "calibrationCurve" | "calibrationError" => return,
            "pauseButtonSetting" => Some(serde_json::to_value(self.job.pause_button()).unwrap_or(Value::Null)),
            _ => {
                if setting == "units" {
                    // Position reports go out only on a change, so the next one is made to count as one.
                    self.position.previous[0] = -99999.0;
                }
                self.settings
                    .value(section, setting)
                    .filter(|v| v.is_string() || v.is_number() || v.is_boolean())
                    .cloned()
            }
        };
        if let Some(value) = answer {
            effects.push(broadcast(&RequestedSetting { setting: setting.into(), value }));
        }
    }
}
