//! The capture task: one thread owns the open camera and fans its frames out, numbered and timed, to every
//! [`Lease`]. The camera opens with the first lease and closes after the last; the live overlay holds one.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use base64::Engine;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use wc_config::Settings;
use wc_machine::{CameraReport, Event, MachineHandle};
use wc_optical::camera::{self, Camera, CameraConfig, SledPosition, Source};
use wc_optical::{Frame, OpticalError};
use wc_protocol::optical::CameraStatus;

const SECTION: &str = "Camera Settings";
/// A frame slower than this means the camera is gone; a few in a row end the capture.
const FRAME_TIMEOUT: Duration = Duration::from_secs(2);
const MISSES_ALLOWED: u32 = 3;
/// The overlay's rate, about 2 fps as Python's.
pub const PREVIEW_PERIOD: Duration = Duration::from_millis(500);
const WATCH_PERIOD: Duration = Duration::from_millis(100);

/// Opens a camera for a source; tests pass a fake.
pub type Opener = Arc<dyn Fn(&Source, Option<SledPosition>) -> wc_optical::Result<Box<dyn Camera>> + Send + Sync>;

/// A frame and when its capture began: it shows nothing older than `at`.
#[derive(Debug, Clone)]
pub struct Captured {
    pub sequence: u64,
    pub at: Instant,
    pub frame: Arc<Frame>,
}

#[derive(Debug, Clone)]
pub enum Feed {
    Starting,
    Frame(Captured),
    Failed(OpticalError),
}

#[derive(Default)]
struct Users {
    count: usize,
    running: bool,
}

struct Shared {
    users: Mutex<Users>,
    feed: watch::Sender<Feed>,
}

impl Shared {
    fn users(&self) -> std::sync::MutexGuard<'_, Users> {
        self.users.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Clone)]
pub struct Cameras {
    sled: Option<SledPosition>,
    opener: Opener,
    shared: Arc<Shared>,
}

impl Default for Cameras {
    fn default() -> Self {
        Cameras::new(None)
    }
}

/// `cameraSource`, `videoSize` and `fps`, read each time the camera starts.
pub fn camera_settings(settings: &Settings) -> (wc_optical::Result<Source>, CameraConfig) {
    let source = settings.text_or_empty(SECTION, "cameraSource").parse();
    let fps = settings.value(SECTION, "fps").and_then(wc_protocol::float).unwrap_or(f64::NAN);
    (source, CameraConfig::from_settings(&settings.text_or_empty(SECTION, "videoSize"), fps))
}

impl Cameras {
    /// `sled` is the simulator's true position, which the synthetic camera looks at.
    pub fn new(sled: Option<SledPosition>) -> Cameras {
        Cameras::with_opener(sled, Arc::new(camera::open))
    }

    pub fn with_opener(sled: Option<SledPosition>, opener: Opener) -> Cameras {
        let shared = Shared { users: Mutex::default(), feed: watch::channel(Feed::Starting).0 };
        Cameras { sled, opener, shared: Arc::new(shared) }
    }

    pub fn sled(&self) -> Option<SledPosition> {
        self.sled.clone()
    }

    /// The capture thread holds the camera.
    pub fn running(&self) -> bool {
        self.shared.users().running
    }

    /// The setting, and whether it names a camera this server could open.
    pub fn status(&self, settings: &Settings) -> CameraStatus {
        let (source, _) = camera_settings(settings);
        let available = match source {
            Ok(Source::Synthetic) => self.sled.is_some(),
            Ok(Source::Dir(dir)) => dir.is_dir(),
            Ok(Source::Device(device)) => cfg!(target_os = "linux") && device.exists(),
            Ok(Source::Auto | Source::Rpicam) => true,
            Err(_) => false,
        };
        CameraStatus { source: settings.text_or_empty(SECTION, "cameraSource"), available }
    }

    /// Opens the camera from `settings` unless it is already open, and shares its frames until dropped.
    pub fn lease(&self, settings: &Settings) -> Lease {
        let mut users = self.shared.users();
        users.count += 1;
        if !users.running {
            users.running = true;
            self.shared.feed.send_replace(Feed::Starting);
            let (source, config) = camera_settings(settings);
            let (opener, sled, shared) = (self.opener.clone(), self.sled.clone(), self.shared.clone());
            let open = move || source.clone().and_then(|source| opener(&source, sled.clone()));
            std::thread::spawn(move || capture(&shared, open, config));
        }
        Lease { shared: self.shared.clone(), feed: self.shared.feed.subscribe() }
    }

    /// Streams the overlay to `machine` while its snapshot asks for it, until the guard drops.
    pub fn stream_to(&self, machine: MachineHandle) -> Streaming {
        Streaming(tokio::spawn(stream(self.clone(), machine)))
    }
}

/// Runs the camera until nobody holds a lease, reopening it if one arrives while it closes.
fn capture(shared: &Shared, open: impl Fn() -> wc_optical::Result<Box<dyn Camera>>, config: CameraConfig) {
    loop {
        let failure = match open().and_then(|mut camera| camera.start(&config).map(|()| camera)) {
            Err(error) => Some(error),
            Ok(mut camera) => {
                let failure = run(shared, camera.as_mut(), config.fps);
                camera.stop();
                failure
            }
        };
        if let Some(error) = &failure {
            shared.feed.send_replace(Feed::Failed(error.clone()));
        }
        let mut users = shared.users();
        if users.count == 0 || failure.is_some() {
            users.running = false;
            return;
        }
    }
}

fn run(shared: &Shared, camera: &mut dyn Camera, fps: f64) -> Option<OpticalError> {
    let period = Duration::from_secs_f64(1.0 / fps);
    let (mut sequence, mut misses) = (0, 0);
    while shared.users().count > 0 {
        let at = Instant::now();
        match camera.next_frame(at + FRAME_TIMEOUT) {
            Ok(frame) => {
                (sequence, misses) = (sequence + 1, 0);
                shared.feed.send_replace(Feed::Frame(Captured { sequence, at, frame: Arc::new(frame) }));
            }
            Err(OpticalError::NoFrame) if misses + 1 < MISSES_ALLOWED => misses += 1,
            Err(error) => return Some(error),
        }
        std::thread::sleep(period.saturating_sub(at.elapsed()));
    }
    None
}

pub struct Lease {
    shared: Arc<Shared>,
    feed: watch::Receiver<Feed>,
}

impl Lease {
    pub fn latest(&self) -> Feed {
        self.feed.borrow().clone()
    }

    /// The first frame whose capture began at or after `after`.
    pub async fn frame_after(&mut self, after: Instant, timeout: Duration) -> wc_optical::Result<Captured> {
        self.wait(timeout, |captured| captured.at >= after).await
    }

    /// `count` successive frames, each begun at or after `after`, as the scan wants after its settle time.
    pub async fn frames_after(
        &mut self,
        count: usize,
        after: Instant,
        timeout: Duration,
    ) -> wc_optical::Result<Vec<Arc<Frame>>> {
        let deadline = Instant::now() + timeout;
        let mut frames: Vec<Captured> = Vec::with_capacity(count);
        while frames.len() < count {
            let newer = frames.last().map_or(0, |last| last.sequence);
            let left = deadline.saturating_duration_since(Instant::now());
            frames.push(self.wait(left, |captured| captured.at >= after && captured.sequence > newer).await?);
        }
        Ok(frames.into_iter().map(|captured| captured.frame).collect())
    }

    async fn wait(&mut self, timeout: Duration, wanted: impl Fn(&Captured) -> bool) -> wc_optical::Result<Captured> {
        let waiting = async {
            loop {
                match &*self.feed.borrow_and_update() {
                    Feed::Frame(captured) if wanted(captured) => return Ok(captured.clone()),
                    Feed::Failed(error) => return Err(error.clone()),
                    _ => {}
                }
                if self.feed.changed().await.is_err() {
                    return Err(OpticalError::NoFrame);
                }
            }
        };
        tokio::time::timeout(timeout, waiting).await.unwrap_or(Err(OpticalError::NoFrame))
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.shared.users().count -= 1;
    }
}

/// The overlay's task; dropping this ends it and releases its lease.
pub struct Streaming(JoinHandle<()>);

impl Drop for Streaming {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn preview(frame: Arc<Frame>) -> Result<String, String> {
    let png = tokio::task::spawn_blocking(move || camera::preview(&frame)).await.map_err(|e| e.to_string())?;
    png.map(|png| base64::engine::general_purpose::STANDARD.encode(png)).map_err(|e| e.to_string())
}

async fn stream(cameras: Cameras, machine: MachineHandle) {
    let (mut lease, mut failed, mut shown, mut due) = (None::<Lease>, false, 0, Instant::now());
    let mut ticks = tokio::time::interval(WATCH_PERIOD);
    loop {
        ticks.tick().await;
        let snapshot = machine.snapshot();
        if !snapshot.camera || failed {
            failed &= snapshot.camera;
            lease = None;
            continue;
        }
        let feed = lease.get_or_insert_with(|| cameras.lease(&snapshot.settings)).latest();
        let report = match feed {
            Feed::Frame(captured) if captured.sequence != shown && Instant::now() >= due => {
                (shown, due) = (captured.sequence, Instant::now() + PREVIEW_PERIOD);
                preview(captured.frame).await.map_or_else(CameraReport::Failed, CameraReport::Frame)
            }
            Feed::Failed(error) => CameraReport::Failed(error.to_string()),
            _ => continue,
        };
        failed = matches!(report, CameraReport::Failed(_));
        machine.send(Event::Camera(report));
    }
}
