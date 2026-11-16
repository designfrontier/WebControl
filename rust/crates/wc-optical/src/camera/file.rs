//! Track B: frames from a directory of PNGs, in name order, looping.

use std::path::{Path, PathBuf};
use std::time::Instant;

use super::synthetic::{Pace, not_started};
use super::{Camera, CameraConfig};
use crate::{Frame, OpticalError, Result};

pub struct DirCamera {
    pub dir: PathBuf,
    frames: Vec<Frame>,
    pace: Option<Pace>,
    next: usize,
}

impl DirCamera {
    pub fn new(dir: &Path) -> DirCamera {
        DirCamera { dir: dir.to_path_buf(), frames: Vec::new(), pace: None, next: 0 }
    }
}

impl Camera for DirCamera {
    /// Decodes every `*.png` up front; the frames keep their own size whatever the config says.
    fn start(&mut self, config: &CameraConfig) -> Result<()> {
        let pace = Pace::new(config)?;
        let failed = |why: String| OpticalError::Camera(format!("{}: {why}", self.dir.display()));
        let mut paths: Vec<PathBuf> = std::fs::read_dir(&self.dir)
            .map_err(|e| failed(e.to_string()))?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("png")))
            .collect();
        paths.sort();
        if paths.is_empty() {
            return Err(failed("no PNG frames".into()));
        }
        self.frames = paths.iter().map(|path| read_png(path).map_err(failed)).collect::<Result<_>>()?;
        (self.pace, self.next) = (Some(pace), 0);
        Ok(())
    }

    fn next_frame(&mut self, deadline: Instant) -> Result<Frame> {
        self.pace.as_mut().ok_or_else(not_started)?.wait(deadline)?;
        let frame = self.frames[self.next % self.frames.len()].clone();
        self.next += 1;
        Ok(frame)
    }

    fn stop(&mut self) {
        (self.frames, self.pace) = (Vec::new(), None);
    }
}

fn read_png(path: &Path) -> std::result::Result<Frame, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .to_luma8();
    Ok(Frame { width: image.width(), height: image.height(), luma: image.into_raw() })
}
