//! Track B: renders the target as seen from the simulated sled's true position.

use std::time::{Duration, Instant};

use super::{Camera, CameraConfig, SledPosition};
use crate::render::{self, Scene, Target};
use crate::{Frame, OpticalError, Result};

pub struct SyntheticCamera {
    pub sled: SledPosition,
    pub target: Target,
    pub scene: Scene,
    pace: Option<Pace>,
    frames: u64,
}

impl SyntheticCamera {
    /// The default target (0.6" squares at 3") and scene; set the fields to change them.
    pub fn new(sled: SledPosition) -> SyntheticCamera {
        let target = Target { marker_mm: (15.24, 15.24), pitch_mm: (crate::grid::PITCH_MM, crate::grid::PITCH_MM) };
        SyntheticCamera { sled, target, scene: Scene::default(), pace: None, frames: 0 }
    }
}

impl Camera for SyntheticCamera {
    /// Takes the frame size from the config; the other scene fields stay as set.
    fn start(&mut self, config: &CameraConfig) -> Result<()> {
        self.pace = Some(Pace::new(config)?);
        (self.scene.width, self.scene.height) = (config.width, config.height);
        Ok(())
    }

    /// Each frame gets fresh noise, as a real sensor's would.
    fn next_frame(&mut self, deadline: Instant) -> Result<Frame> {
        self.pace.as_mut().ok_or_else(not_started)?.wait(deadline)?;
        let scene = Scene { seed: self.scene.seed.wrapping_add(self.frames), ..self.scene };
        self.frames += 1;
        render::render(&self.target, &scene, (self.sled)())
    }

    fn stop(&mut self) {
        self.pace = None;
    }
}

pub(crate) fn not_started() -> OpticalError {
    OpticalError::Camera("the camera is not started".into())
}

/// Hands out frames no faster than `fps`, like a sensor, so a capture loop does not spin.
pub(crate) struct Pace {
    interval: Duration,
    next: Instant,
}

impl Pace {
    pub(crate) fn new(config: &CameraConfig) -> Result<Pace> {
        if !(config.fps > 0.0 && config.fps.is_finite()) || config.width == 0 || config.height == 0 {
            return Err(OpticalError::Invalid(format!(
                "Camera settings {}x{} at {} fps are not usable",
                config.width, config.height, config.fps
            )));
        }
        Ok(Pace { interval: Duration::from_secs_f64(1.0 / config.fps), next: Instant::now() })
    }

    /// Sleeps until the next frame is due, or answers `NoFrame` if that is after `deadline`.
    pub(crate) fn wait(&mut self, deadline: Instant) -> Result<()> {
        let now = Instant::now();
        let due = self.next.max(now);
        if due > deadline {
            return Err(OpticalError::NoFrame);
        }
        std::thread::sleep(due - now);
        self.next = due + self.interval;
        Ok(())
    }
}
