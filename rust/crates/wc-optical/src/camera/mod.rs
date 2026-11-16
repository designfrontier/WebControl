//! Track C: the camera trait, the Camera Source setting and opening a source.

pub mod file;
pub mod rpicam;
pub mod synthetic;
#[cfg(target_os = "linux")]
pub mod v4l2;

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;

use crate::{Frame, OpticalError, Result, pending};

/// From Camera Settings: `videoSize` and `fps`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraConfig {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
}

/// One reader per device; the server's capture task owns it and fans frames out.
pub trait Camera: Send {
    fn start(&mut self, config: &CameraConfig) -> Result<()>;
    /// The next frame captured after this call, or `NoFrame` once `deadline` passes.
    fn next_frame(&mut self, deadline: Instant) -> Result<Frame>;
    fn stop(&mut self);
}

/// The simulated sled's true position in machine mm, which the synthetic camera looks at.
pub type SledPosition = Arc<dyn Fn() -> (f64, f64) + Send + Sync>;

/// The Camera Settings `cameraSource` setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// The Pi camera if `rpicam-hello --list-cameras` lists one, else the first V4L2 capture device.
    Auto,
    Rpicam,
    Device(PathBuf),
    Dir(PathBuf),
    Synthetic,
}

impl FromStr for Source {
    type Err = OpticalError;

    fn from_str(text: &str) -> Result<Source> {
        match text.trim() {
            "" | "auto" => Ok(Source::Auto),
            "rpicam" => Ok(Source::Rpicam),
            "synthetic" => Ok(Source::Synthetic),
            device if device.starts_with("/dev/") => Ok(Source::Device(device.into())),
            other => other.strip_prefix("dir:").map(|dir| Source::Dir(dir.into())).ok_or_else(|| {
                OpticalError::Invalid(format!(
                    "Camera Source {other:?} is not auto, rpicam, /dev/videoN, dir:PATH or synthetic"
                ))
            }),
        }
    }
}

/// `sled` is the simulator's position, which only `--simulate` has.
pub fn open(source: &Source, sled: Option<SledPosition>) -> Result<Box<dyn Camera>> {
    match source {
        Source::Auto => pending("camera::open(auto)"),
        Source::Rpicam => Ok(Box::new(rpicam::RpicamCamera::new())),
        Source::Device(path) => device(path),
        Source::Dir(path) => Ok(Box::new(file::DirCamera::new(path))),
        Source::Synthetic => match sled {
            Some(sled) => Ok(Box::new(synthetic::SyntheticCamera::new(sled))),
            None => Err(OpticalError::Invalid("The synthetic camera needs webcontrol-rs --simulate".into())),
        },
    }
}

#[cfg(target_os = "linux")]
fn device(path: &Path) -> Result<Box<dyn Camera>> {
    Ok(Box::new(v4l2::V4l2Camera::new(path)))
}

#[cfg(not(target_os = "linux"))]
fn device(path: &Path) -> Result<Box<dyn Camera>> {
    Err(OpticalError::Camera(format!("{} needs Linux's V4L2", path.display())))
}
