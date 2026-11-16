//! The capture task against a fake camera: one camera shared by every lease, numbered and timed frames, opening on
//! demand and closing when idle, failures, and the live overlay it streams to the UI at about 2 fps.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine;
use serde_json::{Value, json};
use wc_config::Settings;
use wc_machine::files::Request;
use wc_machine::{Event, Machine, MachineHandle, NoHost, NoPort, spawn};
use wc_optical::camera::{Camera, CameraConfig, SledPosition, Source};
use wc_optical::{Frame, OpticalError};
use wc_protocol::{Action, Push};
use wc_server::camera::{Cameras, Feed, Opener, PREVIEW_PERIOD, camera_settings};
use wc_testkit::contract::Contract;

#[derive(Default)]
struct Log {
    opened: Vec<Source>,
    configs: Vec<CameraConfig>,
    stops: usize,
}

#[derive(Clone, Default)]
struct Behaviour {
    /// How long each frame takes; the call blocks that long, as a real camera's read does.
    frame_time: Duration,
    refuse_start: Option<String>,
    no_frames: bool,
}

struct Fake {
    log: Arc<Mutex<Log>>,
    behaviour: Behaviour,
    shade: u8,
}

impl Camera for Fake {
    fn start(&mut self, config: &CameraConfig) -> wc_optical::Result<()> {
        self.log.lock().unwrap().configs.push(*config);
        self.behaviour.refuse_start.clone().map_or(Ok(()), |why| Err(OpticalError::Camera(why)))
    }

    fn next_frame(&mut self, _deadline: Instant) -> wc_optical::Result<Frame> {
        std::thread::sleep(self.behaviour.frame_time);
        if self.behaviour.no_frames {
            return Err(OpticalError::NoFrame);
        }
        self.shade = self.shade.wrapping_add(1);
        Ok(Frame { width: 64, height: 48, luma: vec![self.shade; 64 * 48] })
    }

    fn stop(&mut self) {
        self.log.lock().unwrap().stops += 1;
    }
}

fn fake(behaviour: Behaviour) -> (Opener, Arc<Mutex<Log>>) {
    let log = Arc::new(Mutex::new(Log::default()));
    let opened = log.clone();
    let opener: Opener = Arc::new(move |source: &Source, _sled: Option<SledPosition>| {
        opened.lock().unwrap().opened.push(source.clone());
        Ok(Box::new(Fake { log: opened.clone(), behaviour: behaviour.clone(), shade: 0 }) as Box<dyn Camera>)
    });
    (opener, log)
}

fn quick() -> Behaviour {
    Behaviour { frame_time: Duration::from_millis(10), ..Behaviour::default() }
}

fn settings(changes: &[(&str, &str, Value)]) -> Settings {
    let mut settings = Settings::defaults();
    changes.iter().for_each(|(section, key, value)| assert!(settings.set_value(section, key, value.clone()), "{key}"));
    settings
}

fn camera_setting(key: &str, value: Value) -> (&'static str, &str, Value) {
    ("Camera Settings", key, value)
}

async fn eventually(what: &str, done: impl Fn() -> bool) {
    let waited = async {
        while !done() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(5), waited).await.unwrap_or_else(|_| panic!("never: {what}"));
}

const SOON: Duration = Duration::from_secs(3);

#[tokio::test]
async fn leases_share_one_camera_and_it_closes_after_the_last() {
    let (opener, log) = fake(quick());
    let cameras = Cameras::with_opener(None, opener);
    let wanted = settings(&[camera_setting("fps", json!(30))]);
    let (mut scan, mut overlay) = (cameras.lease(&wanted), cameras.lease(&wanted));
    let frames = scan.frames_after(3, Instant::now(), SOON).await.unwrap();
    let shades: Vec<u8> = frames.iter().map(|frame| frame.luma[0]).collect();
    assert!(shades.windows(2).all(|pair| pair[0] < pair[1]), "successive frames, never one twice: {shades:?}");
    let first = overlay.frame_after(Instant::now(), SOON).await.unwrap();
    let next = overlay.frame_after(Instant::now(), SOON).await.unwrap();
    assert!(next.sequence > first.sequence && next.at > first.at);
    assert_eq!(log.lock().unwrap().opened, [Source::Auto], "one camera for both");
    drop(scan);
    assert!(cameras.running(), "the overlay still holds it");
    drop(overlay);
    eventually("the camera closes", || !cameras.running()).await;
    assert_eq!(log.lock().unwrap().stops, 1);
    let mut again = cameras.lease(&wanted);
    again.frame_after(Instant::now(), SOON).await.unwrap();
    assert_eq!(log.lock().unwrap().opened.len(), 2, "the next lease opens it again");
}

#[tokio::test]
async fn a_frame_begun_before_the_settle_deadline_is_never_taken() {
    let (opener, _log) = fake(Behaviour { frame_time: Duration::from_millis(60), ..Behaviour::default() });
    let cameras = Cameras::with_opener(None, opener);
    let mut lease = cameras.lease(&Settings::defaults());
    let settled = Instant::now() + Duration::from_millis(250);
    let captured = lease.frame_after(settled, SOON).await.unwrap();
    assert!(captured.at >= settled, "{:?} before the deadline", settled - captured.at);
    assert!(matches!(lease.latest(), Feed::Frame(_)));
}

#[tokio::test]
async fn the_camera_settings_choose_the_source_size_and_rate() {
    let (opener, log) = fake(quick());
    let cameras = Cameras::with_opener(None, opener);
    let chosen = settings(&[
        camera_setting("cameraSource", json!("dir:/frames")),
        camera_setting("videoSize", json!("1024x768")),
        camera_setting("fps", json!(2)),
    ]);
    let mut lease = cameras.lease(&chosen);
    lease.frame_after(Instant::now(), SOON).await.unwrap();
    let log = log.lock().unwrap();
    assert_eq!(log.opened, [Source::Dir("/frames".into())]);
    assert_eq!(log.configs, [CameraConfig { width: 1024, height: 768, fps: 2.0 }]);
    let (source, config) = camera_settings(&Settings::defaults());
    assert_eq!((source, config), (Ok(Source::Auto), CameraConfig { width: 640, height: 480, fps: 5.0 }));
}

#[tokio::test]
async fn a_camera_that_will_not_open_fails_its_lease() {
    let cameras = Cameras::new(None);
    let mut synthetic = cameras.lease(&settings(&[camera_setting("cameraSource", json!("synthetic"))]));
    let refused = synthetic.frame_after(Instant::now(), SOON).await.unwrap_err();
    assert_eq!(refused.to_string(), "The synthetic camera needs webcontrol-rs --simulate");
    eventually("the capture thread ends", || !cameras.running()).await;
    drop(synthetic);
    let mut wrong = cameras.lease(&settings(&[camera_setting("cameraSource", json!("webcam"))]));
    let refused = wrong.frame_after(Instant::now(), SOON).await.unwrap_err().to_string();
    assert!(refused.starts_with("Camera Source \"webcam\" is not auto"), "{refused}");
    let (opener, _) = fake(Behaviour { refuse_start: Some("busy".into()), ..quick() });
    let mut busy = Cameras::with_opener(None, opener).lease(&Settings::defaults());
    assert_eq!(busy.frames_after(2, Instant::now(), SOON).await, Err(OpticalError::Camera("busy".into())));
}

#[tokio::test]
async fn a_camera_that_stops_giving_frames_fails_after_a_few_misses() {
    let (opener, log) = fake(Behaviour { no_frames: true, ..quick() });
    let cameras = Cameras::with_opener(None, opener);
    let mut lease = cameras.lease(&settings(&[camera_setting("fps", json!(30))]));
    let gone = lease.frame_after(Instant::now(), SOON).await.unwrap_err();
    assert_eq!(gone, OpticalError::NoFrame);
    eventually("the camera closes", || !cameras.running()).await;
    assert_eq!(log.lock().unwrap().stops, 1);
}

#[tokio::test]
async fn a_lease_gives_up_at_its_timeout() {
    let (opener, _) = fake(Behaviour { frame_time: Duration::from_millis(400), ..Behaviour::default() });
    let mut lease = Cameras::with_opener(None, opener).lease(&Settings::defaults());
    let asked = Instant::now();
    assert_eq!(lease.frame_after(asked, Duration::from_millis(100)).await.unwrap_err(), OpticalError::NoFrame);
    assert!(asked.elapsed() < Duration::from_millis(350));
}

#[test]
fn the_status_says_whether_the_source_could_open_here() {
    let sled: SledPosition = Arc::new(|| (0.0, 0.0));
    let status = |cameras: &Cameras, source: &str| {
        let status = cameras.status(&settings(&[camera_setting("cameraSource", json!(source))]));
        assert_eq!(status.source, source);
        status.available
    };
    let (plain, simulated) = (Cameras::default(), Cameras::new(Some(sled)));
    assert!(!status(&plain, "synthetic") && status(&simulated, "synthetic"));
    let here = std::env::temp_dir().display().to_string();
    assert!(status(&plain, &format!("dir:{here}")) && !status(&plain, "dir:/no/such/frames"));
    assert!(status(&plain, "auto") && status(&plain, "rpicam") && !status(&plain, "webcam"));
    assert!(!status(&plain, "/dev/video99"));
}

fn optical_machine(on: bool) -> MachineHandle {
    let enabled = settings(&[("WebControl Settings", "opticalCalibrationEnabled", json!(i32::from(on)))]);
    spawn(Machine::new("10.0.0.5:5000", enabled), Box::new(NoPort), Arc::new(NoHost)).0
}

fn act(machine: &MachineHandle, command: &str) {
    machine.send(Event::Action(Action::new(command, json!(""), json!(""))));
}

/// The next `cameraMessage` or alert, checked against the contract, or `None` after `within`.
async fn next_camera_push(pushes: &mut tokio::sync::broadcast::Receiver<Push>, within: Duration) -> Option<Value> {
    let wanted = async {
        loop {
            let push = pushes.recv().await.unwrap();
            let command = push.data["command"].as_str().unwrap_or_default().to_string();
            if command == "cameraMessage" || command == "activateModal" {
                Contract::shared().validate_frame(&push.frame().encode()).unwrap();
                return serde_json::from_str(push.data["data"].as_str().unwrap()).unwrap();
            }
        }
    };
    tokio::time::timeout(within, wanted).await.ok()
}

fn png_size(data: &str) -> (u32, u32) {
    let png = base64::engine::general_purpose::STANDARD.decode(data).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    let word = |at: usize| u32::from_be_bytes(png[at..at + 4].try_into().unwrap());
    (word(16), word(20))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_overlay_streams_small_frames_at_about_two_a_second_until_toggled_off() {
    let machine = optical_machine(true);
    let (opener, log) = fake(quick());
    let cameras = Cameras::with_opener(None, opener);
    let _streaming = cameras.stream_to(machine.clone());
    let mut pushes = machine.subscribe();
    act(&machine, "toggleCamera");
    assert_eq!(next_camera_push(&mut pushes, SOON).await, Some(json!({"command": "updateCamera", "data": "on"})));
    let mut arrivals = Vec::new();
    while arrivals.len() < 3 {
        let image = next_camera_push(&mut pushes, SOON).await.expect("a frame");
        assert_eq!(image["command"], "cameraImageUpdated");
        assert_eq!(png_size(image["data"].as_str().unwrap()), (256, 192));
        arrivals.push(Instant::now());
    }
    let gaps: Vec<Duration> = arrivals.windows(2).map(|pair| pair[1] - pair[0]).collect();
    assert!(gaps.iter().all(|gap| *gap >= PREVIEW_PERIOD - Duration::from_millis(100)), "{gaps:?}");
    assert!(gaps.iter().all(|gap| *gap < PREVIEW_PERIOD * 3), "{gaps:?}");
    act(&machine, "toggleCamera");
    let off = async {
        while next_camera_push(&mut pushes, SOON).await != Some(json!({"command": "updateCamera", "data": "off"})) {}
    };
    tokio::time::timeout(SOON, off).await.expect("updateCamera off");
    eventually("the camera closes when the overlay is off", || !cameras.running()).await;
    let log = log.lock().unwrap();
    assert_eq!((log.opened.len(), log.stops), (1, 1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_camera_blocked_on_a_frame_never_holds_up_the_machine() {
    let machine = optical_machine(true);
    let (opener, _) = fake(Behaviour { frame_time: Duration::from_millis(1500), ..Behaviour::default() });
    let cameras = Cameras::with_opener(None, opener);
    let _streaming = cameras.stream_to(machine.clone());
    act(&machine, "toggleCamera");
    eventually("the camera is reading", || cameras.running()).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let asked = Instant::now();
    machine.request(Request::SetSettings(Vec::new())).await;
    assert!(asked.elapsed() < Duration::from_millis(200), "the machine answered in {:?}", asked.elapsed());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_camera_that_fails_turns_the_overlay_off_and_toggling_tries_again() {
    let machine = optical_machine(true);
    let (opener, log) = fake(Behaviour { refuse_start: Some("/dev/video0: busy".into()), ..quick() });
    let cameras = Cameras::with_opener(None, opener);
    let _streaming = cameras.stream_to(machine.clone());
    let mut pushes = machine.subscribe();
    act(&machine, "toggleCamera");
    assert_eq!(next_camera_push(&mut pushes, SOON).await.unwrap()["data"], "on");
    assert_eq!(next_camera_push(&mut pushes, SOON).await.unwrap()["data"], "off");
    assert_eq!(next_camera_push(&mut pushes, SOON).await.unwrap()["message"], "Camera: /dev/video0: busy");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(log.lock().unwrap().opened.len(), 1, "a failed camera is not retried until toggled");
    act(&machine, "toggleCamera");
    eventually("toggling opens it again", || log.lock().unwrap().opened.len() == 2).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nothing_opens_while_optical_calibration_is_off() {
    let machine = optical_machine(false);
    let (opener, log) = fake(quick());
    let cameras = Cameras::with_opener(None, opener);
    let _streaming = cameras.stream_to(machine.clone());
    let mut pushes = machine.subscribe();
    act(&machine, "toggleCamera");
    assert_eq!(next_camera_push(&mut pushes, Duration::from_millis(400)).await, None);
    assert!(log.lock().unwrap().opened.is_empty() && !cameras.running());
}
