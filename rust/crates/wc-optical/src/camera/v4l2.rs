//! USB/UVC webcams over V4L2 (mmap streaming; YUYV preferred, else MJPEG). Linux only.
//! Hardware glue, left out of the coverage floor (`scripts/coverage.sh`).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use v4l::buffer::Type;
use v4l::capability::Flags;
use v4l::io::mmap::Stream;
use v4l::io::traits::CaptureStream;
use v4l::video::Capture;
use v4l::video::capture::Parameters;
use v4l::{Device, FourCC};

use super::{Camera, CameraConfig};
use crate::frame::PixelFormat;
use crate::{Frame, OpticalError, Result};

const BUFFERS: u32 = 4;
const FORMATS: [(&[u8; 4], PixelFormat); 2] = [(b"YUYV", PixelFormat::Yuyv), (b"MJPG", PixelFormat::Jpeg)];

/// What the driver agreed to: the pixel format, the size and the bytes per YUYV row.
#[derive(Debug, Clone, Copy)]
struct Negotiated {
    format: PixelFormat,
    width: u32,
    height: u32,
    stride: u32,
}

pub struct V4l2Camera {
    pub device: PathBuf,
    stream: Option<Stream<'static>>,
    negotiated: Option<Negotiated>,
}

impl V4l2Camera {
    pub fn new(device: &Path) -> V4l2Camera {
        V4l2Camera { device: device.to_path_buf(), stream: None, negotiated: None }
    }

    fn failed(&self, error: impl std::fmt::Display) -> OpticalError {
        OpticalError::Camera(format!("{}: {error}", self.device.display()))
    }
}

fn negotiate(device: &Device, config: &CameraConfig) -> Option<Negotiated> {
    FORMATS.iter().find_map(|(fourcc, format)| {
        let mut wanted = device.format().ok()?;
        (wanted.width, wanted.height, wanted.fourcc) = (config.width, config.height, FourCC::new(fourcc));
        let got = device.set_format(&wanted).ok().filter(|got| got.fourcc == FourCC::new(fourcc))?;
        Some(Negotiated { format: *format, width: got.width, height: got.height, stride: got.stride })
    })
}

/// YUYV rows without the driver's padding.
fn packed(bytes: &[u8], negotiated: &Negotiated) -> Vec<u8> {
    let row = negotiated.width as usize * 2;
    match negotiated.stride as usize {
        stride if stride > row => bytes.chunks(stride).flat_map(|line| &line[..row.min(line.len())]).copied().collect(),
        _ => bytes.to_vec(),
    }
}

impl Camera for V4l2Camera {
    fn start(&mut self, config: &CameraConfig) -> Result<()> {
        self.stop();
        let device = Device::with_path(&self.device).map_err(|error| self.failed(error))?;
        let negotiated = negotiate(&device, config).ok_or_else(|| self.failed("offers neither YUYV nor MJPEG"))?;
        let _ = device.set_params(&Parameters::with_fps(config.fps.round().max(1.0) as u32));
        let stream = Stream::with_buffers(&device, Type::VideoCapture, BUFFERS).map_err(|error| self.failed(error))?;
        (self.stream, self.negotiated) = (Some(stream), Some(negotiated));
        Ok(())
    }

    fn next_frame(&mut self, deadline: Instant) -> Result<Frame> {
        let (Some(stream), Some(negotiated)) = (self.stream.as_mut(), self.negotiated) else {
            return Err(OpticalError::Camera(format!("{} is not started", self.device.display())));
        };
        // Frames already waiting were captured before this call.
        stream.set_timeout(Duration::ZERO);
        (0..BUFFERS).take_while(|_| stream.next().is_ok()).for_each(drop);
        stream.set_timeout(deadline.saturating_duration_since(Instant::now()).max(Duration::from_millis(1)));
        let (buffer, meta) = match stream.next() {
            Ok(next) => next,
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => return Err(OpticalError::NoFrame),
            Err(error) => return Err(OpticalError::Camera(format!("{}: {error}", self.device.display()))),
        };
        let bytes = &buffer[..(meta.bytesused as usize).min(buffer.len())];
        let (width, height) = (negotiated.width, negotiated.height);
        match negotiated.format {
            PixelFormat::Yuyv => Frame::decode(PixelFormat::Yuyv, width, height, &packed(bytes, &negotiated)),
            format => Frame::decode(format, width, height, bytes),
        }
    }

    fn stop(&mut self) {
        self.stream = None;
        self.negotiated = None;
    }
}

/// `/dev/video*` nodes that capture video over USB, in number order; the Pi's codec and ISP nodes are not.
pub fn capture_devices() -> Vec<PathBuf> {
    let number = |path: &Path| path.file_name()?.to_str()?.strip_prefix("video")?.parse::<u32>().ok();
    let mut nodes: Vec<(u32, PathBuf)> = std::fs::read_dir("/dev")
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter_map(|path| number(&path).map(|n| (n, path)))
        .collect();
    nodes.sort();
    nodes
        .into_iter()
        .map(|(_, path)| path)
        .filter(|path| {
            let caps = Device::with_path(path).and_then(|device| device.query_caps());
            caps.is_ok_and(|caps| caps.capabilities.contains(Flags::VIDEO_CAPTURE) && caps.bus.starts_with("usb-"))
        })
        .collect()
}
