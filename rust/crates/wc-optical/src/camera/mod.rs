//! The camera trait, the Camera Source setting, opening a source, and the pieces the adapters share.

pub mod file;
pub mod rpicam;
pub mod synthetic;
#[cfg(target_os = "linux")]
pub mod v4l2;

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;

use image::ImageEncoder;
use image::codecs::png::PngEncoder;
use image::imageops::{FilterType, resize};

use crate::{Frame, OpticalError, Result};

/// The live overlay's size, as Python's capture thread sent it.
pub const PREVIEW_SIZE: (u32, u32) = (256, 192);
const DEFAULT_SIZE: (u32, u32) = (640, 480);
const DEFAULT_FPS: f64 = 5.0;
const MAX_FPS: f64 = 30.0;
/// Past this a frame no longer fits a Pi 3B's memory ten times over.
const MAX_SIDE: u32 = 4096;

/// From Camera Settings: `videoSize` and `fps`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraConfig {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
}

impl CameraConfig {
    /// `videoSize` is `WIDTHxHEIGHT`; anything unreadable or wider than 4096 falls back to 640x480 at 5 fps.
    pub fn from_settings(video_size: &str, fps: f64) -> CameraConfig {
        let size = video_size
            .split_once('x')
            .and_then(|(w, h)| Some((w.trim().parse().ok()?, h.trim().parse().ok()?)))
            .filter(|&(w, h): &(u32, u32)| (1..=MAX_SIDE).contains(&w) && (1..=MAX_SIDE).contains(&h))
            .unwrap_or(DEFAULT_SIZE);
        let fps = if fps.is_finite() && fps > 0.0 { fps.min(MAX_FPS) } else { DEFAULT_FPS };
        CameraConfig { width: size.0, height: size.1, fps }
    }
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
        Source::Auto => open(&pick_auto(rpicam::list_cameras().as_deref(), &capture_devices())?, sled),
        Source::Rpicam => Ok(Box::new(rpicam::RpicamCamera::new())),
        Source::Device(path) => device(path),
        Source::Dir(path) => Ok(Box::new(file::DirCamera::new(path))),
        Source::Synthetic => match sled {
            Some(sled) => Ok(Box::new(synthetic::SyntheticCamera::new(sled))),
            None => Err(OpticalError::Invalid("The synthetic camera needs webcontrol-rs --simulate".into())),
        },
    }
}

/// `auto`: the Pi camera if `listing` (`rpicam-hello --list-cameras`) names one, else the first USB webcam.
pub fn pick_auto(listing: Option<&str>, devices: &[PathBuf]) -> Result<Source> {
    let numbered = |line: &str| line.trim_start().split_once(" : ").is_some_and(|(n, _)| n.parse::<u32>().is_ok());
    match (listing.is_some_and(|text| text.lines().any(numbered)), devices.first()) {
        (true, _) => Ok(Source::Rpicam),
        (false, Some(device)) => Ok(Source::Device(device.clone())),
        (false, None) => Err(OpticalError::Camera("no Pi camera (rpicam-hello lists none) and no USB webcam".into())),
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

#[cfg(target_os = "linux")]
fn capture_devices() -> Vec<PathBuf> {
    v4l2::capture_devices()
}

#[cfg(not(target_os = "linux"))]
fn capture_devices() -> Vec<PathBuf> {
    Vec::new()
}

/// The frame shrunk to [`PREVIEW_SIZE`] as a PNG, for the live overlay.
pub fn preview(frame: &Frame) -> Result<Vec<u8>> {
    let invalid = || OpticalError::Invalid(format!("a {}x{} frame without its pixels", frame.width, frame.height));
    let image = image::GrayImage::from_raw(frame.width, frame.height, frame.luma.clone()).ok_or_else(invalid)?;
    let small = resize(&image, PREVIEW_SIZE.0, PREVIEW_SIZE.1, FilterType::Triangle);
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(small.as_raw(), PREVIEW_SIZE.0, PREVIEW_SIZE.1, image::ExtendedColorType::L8)
        .map_err(|error| OpticalError::Invalid(error.to_string()))?;
    Ok(png)
}

/// Cuts an MJPEG byte stream into whole JPEGs at each SOI (`FF D8`) and EOI (`FF D9`).
#[derive(Debug, Default)]
pub struct MjpegSplitter {
    pending: Vec<u8>,
}

impl MjpegSplitter {
    /// Every JPEG `bytes` completes; a partial one waits for the next call.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        self.pending.extend_from_slice(bytes);
        let mut jpegs = Vec::new();
        while let Some(jpeg) = self.next_jpeg() {
            jpegs.push(jpeg);
        }
        jpegs
    }

    fn next_jpeg(&mut self) -> Option<Vec<u8>> {
        let Some(start) = find(&self.pending, [0xFF, 0xD8], 0) else {
            let keep = usize::from(self.pending.last() == Some(&0xFF));
            self.pending.drain(..self.pending.len() - keep);
            return None;
        };
        self.pending.drain(..start);
        let end = find(&self.pending, [0xFF, 0xD9], 2)? + 2;
        Some(self.pending.drain(..end).collect())
    }
}

fn find(bytes: &[u8], marker: [u8; 2], from: usize) -> Option<usize> {
    bytes.get(from..)?.windows(2).position(|pair| pair == marker).map(|at| at + from)
}
