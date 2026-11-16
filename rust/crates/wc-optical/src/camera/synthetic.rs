//! Track B: renders the target as seen from the simulated sled's true position.

use std::time::Instant;

use super::{Camera, CameraConfig, SledPosition};
use crate::render::{Scene, Target};
use crate::{Frame, Result, pending};

pub struct SyntheticCamera {
    pub sled: SledPosition,
    pub target: Target,
    pub scene: Scene,
}

impl SyntheticCamera {
    /// The default target (0.6" squares at 3") and scene; set the fields to change them.
    pub fn new(sled: SledPosition) -> SyntheticCamera {
        let target = Target { marker_mm: (15.24, 15.24), pitch_mm: (crate::grid::PITCH_MM, crate::grid::PITCH_MM) };
        SyntheticCamera { sled, target, scene: Scene::default() }
    }
}

impl Camera for SyntheticCamera {
    fn start(&mut self, _config: &CameraConfig) -> Result<()> {
        pending("SyntheticCamera::start")
    }

    fn next_frame(&mut self, _deadline: Instant) -> Result<Frame> {
        pending("SyntheticCamera::next_frame")
    }

    fn stop(&mut self) {}
}
