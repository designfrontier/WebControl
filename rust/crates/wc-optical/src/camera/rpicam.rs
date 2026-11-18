//! The Pi camera module through `rpicam-vid --codec mjpeg -o -`, split into JPEGs on SOI/EOI.
//! Hardware glue, left out of the coverage floor (`scripts/coverage.sh`).

use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Instant;

use super::{Camera, CameraConfig, MjpegSplitter};
use crate::frame::PixelFormat;
use crate::{Frame, OpticalError, Result};

const STDERR_KEPT: usize = 400;

/// The newest whole JPEG; the reader keeps only that, so the pipe never backs up with stale frames.
#[derive(Default)]
struct Latest {
    sequence: u64,
    jpeg: Vec<u8>,
    ended: bool,
    stderr: String,
    stderr_read: bool,
}

type Shared = Arc<(Mutex<Latest>, Condvar)>;

pub struct RpicamCamera {
    /// `rpicam-vid`, found on `PATH`; tests point it at a fake.
    pub program: PathBuf,
    child: Option<Child>,
    shared: Shared,
    size: (u32, u32),
}

impl RpicamCamera {
    pub fn new() -> RpicamCamera {
        RpicamCamera { program: "rpicam-vid".into(), child: None, shared: Shared::default(), size: (0, 0) }
    }

    /// The next whole JPEG to arrive after this call.
    pub fn next_jpeg(&mut self, deadline: Instant) -> Result<Vec<u8>> {
        let (lock, arrived) = &*self.shared;
        let mut latest = lock.lock().unwrap_or_else(PoisonError::into_inner);
        let seen = latest.sequence;
        loop {
            if latest.sequence > seen {
                return Ok(latest.jpeg.clone());
            }
            // Both pipes close when it exits; waiting for stderr too keeps its reason in the error.
            if latest.ended && latest.stderr_read {
                let why = latest.stderr.trim();
                return Err(OpticalError::Camera(format!("{} stopped: {why}", self.program.display())));
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(OpticalError::NoFrame);
            }
            latest = arrived.wait_timeout(latest, left).unwrap_or_else(PoisonError::into_inner).0;
        }
    }
}

impl Default for RpicamCamera {
    fn default() -> Self {
        RpicamCamera::new()
    }
}

fn update(shared: &Shared, change: impl FnOnce(&mut Latest)) {
    change(&mut shared.0.lock().unwrap_or_else(PoisonError::into_inner));
    shared.1.notify_all();
}

fn read_frames(mut stdout: impl Read, shared: Shared) {
    let (mut splitter, mut chunk) = (MjpegSplitter::default(), vec![0; 64 * 1024]);
    while let Ok(read @ 1..) = stdout.read(&mut chunk) {
        if let Some(jpeg) = splitter.push(&chunk[..read]).pop() {
            update(&shared, |latest| {
                latest.sequence += 1;
                latest.jpeg = jpeg;
            });
        }
    }
    update(&shared, |latest| latest.ended = true);
}

fn read_stderr(mut stderr: impl Read, shared: Shared) {
    let mut text = String::new();
    let _ = stderr.read_to_string(&mut text);
    let tail = text.char_indices().rev().nth(STDERR_KEPT).map_or(0, |(at, _)| at);
    update(&shared, |latest| {
        latest.stderr = text[tail..].to_string();
        latest.stderr_read = true;
    });
}

impl Camera for RpicamCamera {
    fn start(&mut self, config: &CameraConfig) -> Result<()> {
        self.stop();
        let args = [
            "--codec".to_string(),
            "mjpeg".into(),
            "--width".into(),
            config.width.to_string(),
            "--height".into(),
            config.height.to_string(),
            "--framerate".into(),
            config.fps.to_string(),
            "-t".into(),
            "0".into(),
            "--nopreview".into(),
            "-o".into(),
            "-".into(),
        ];
        let mut child = Command::new(&self.program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| OpticalError::Camera(format!("{}: {error}", self.program.display())))?;
        self.shared = Shared::default();
        let (stdout, stderr) = (child.stdout.take(), child.stderr.take());
        let (out, err) = (self.shared.clone(), self.shared.clone());
        // Each reader ends when its pipe closes; a restart gives the new ones a fresh `Shared`.
        std::thread::spawn(move || stdout.map_or((), |stdout| read_frames(stdout, out)));
        std::thread::spawn(move || match stderr {
            Some(stderr) => read_stderr(stderr, err),
            None => update(&err, |latest| latest.stderr_read = true),
        });
        self.child = Some(child);
        self.size = (config.width, config.height);
        Ok(())
    }

    fn next_frame(&mut self, deadline: Instant) -> Result<Frame> {
        let jpeg = self.next_jpeg(deadline)?;
        Frame::decode(PixelFormat::Jpeg, self.size.0, self.size.1, &jpeg)
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for RpicamCamera {
    fn drop(&mut self) {
        self.stop();
    }
}

/// What `rpicam-hello --list-cameras` prints, or `None` where it is not installed.
pub fn list_cameras() -> Option<String> {
    let output = Command::new("rpicam-hello").arg("--list-cameras").stdin(Stdio::null()).output().ok()?;
    Some(format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr)))
}
