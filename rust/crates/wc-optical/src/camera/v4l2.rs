//! Track C: USB/UVC webcams over V4L2 (mmap streaming; YUYV preferred, else MJPEG). Linux only.
//! Hardware glue, left out of the coverage floor (`scripts/coverage.sh`).

use std::path::{Path, PathBuf};
use std::time::Instant;

use super::{Camera, CameraConfig};
use crate::{Frame, Result, pending};

pub struct V4l2Camera {
    pub device: PathBuf,
}

impl V4l2Camera {
    pub fn new(device: &Path) -> V4l2Camera {
        V4l2Camera { device: device.to_path_buf() }
    }
}

impl Camera for V4l2Camera {
    fn start(&mut self, _config: &CameraConfig) -> Result<()> {
        pending("V4l2Camera::start")
    }

    fn next_frame(&mut self, _deadline: Instant) -> Result<Frame> {
        pending("V4l2Camera::next_frame")
    }

    fn stop(&mut self) {}
}
