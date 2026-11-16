//! Track B: frames from a directory of PNGs, in name order, looping.

use std::path::{Path, PathBuf};
use std::time::Instant;

use super::{Camera, CameraConfig};
use crate::{Frame, Result, pending};

pub struct DirCamera {
    pub dir: PathBuf,
}

impl DirCamera {
    pub fn new(dir: &Path) -> DirCamera {
        DirCamera { dir: dir.to_path_buf() }
    }
}

impl Camera for DirCamera {
    fn start(&mut self, _config: &CameraConfig) -> Result<()> {
        pending("DirCamera::start")
    }

    fn next_frame(&mut self, _deadline: Instant) -> Result<Frame> {
        pending("DirCamera::next_frame")
    }

    fn stop(&mut self) {}
}
