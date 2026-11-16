//! Track C: the Pi camera module through `rpicam-vid --codec mjpeg -o -`, split into JPEGs on SOI/EOI.
//! Hardware glue, left out of the coverage floor (`scripts/coverage.sh`).

use std::path::PathBuf;
use std::time::Instant;

use super::{Camera, CameraConfig};
use crate::{Frame, Result, pending};

pub struct RpicamCamera {
    /// `rpicam-vid`, found on `PATH`; tests point it at a fake.
    pub program: PathBuf,
}

impl RpicamCamera {
    pub fn new() -> RpicamCamera {
        RpicamCamera { program: "rpicam-vid".into() }
    }
}

impl Default for RpicamCamera {
    fn default() -> Self {
        RpicamCamera::new()
    }
}

impl Camera for RpicamCamera {
    fn start(&mut self, _config: &CameraConfig) -> Result<()> {
        pending("RpicamCamera::start")
    }

    fn next_frame(&mut self, _deadline: Instant) -> Result<Frame> {
        pending("RpicamCamera::next_frame")
    }

    fn stop(&mut self) {}
}
