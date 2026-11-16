//! `/api/calibration/optical*`: 409 while the setting is off. A scan's camera work is served here, off the machine
//! task, for as long as the scan runs.

use std::time::{Duration, Instant};

use axum::Json;
use axum::Router;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use base64::Engine;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::sync::broadcast;
use wc_machine::optical::{self, Seen, Vision};
use wc_machine::{Event, MachineHandle, Request};
use wc_optical::detect::{DetectParams, Mode};
use wc_optical::measure::FRAMES;
use wc_optical::{Frame, Measurement, OpticalSettings, Orientation, PixelPoint};
use wc_protocol::optical::{CaptureRequest, CenterFound, OpticalState, ScanRequest, TargetQuery, TestImage};

use crate::AppState;
use crate::camera::{Cameras, Lease};

/// How long a measurement's frames may take to arrive.
const CAPTURE_WAIT: Duration = Duration::from_secs(10);
/// How often a scan's camera server checks that its scan still runs.
const POLL: Duration = Duration::from_millis(250);

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/calibration/optical", get(current))
        .route("/api/calibration/optical/settings", put(save))
        .route("/api/calibration/optical/scan", post(scan))
        .route("/api/calibration/optical/test-image", post(test_image))
        .route("/api/calibration/optical/find-center", post(find_center))
        .route("/api/calibration/optical/fit", post(fit))
        .route("/api/calibration/optical/accept", post(accept))
        .route("/api/calibration/optical/csv", get(csv))
        .route("/api/calibration/optical/target.pdf", get(target))
}

/// The 409 every optical route answers while the setting is off.
pub(crate) fn refused(state: &AppState) -> Option<Response> {
    let off = !optical::enabled(&state.machine.snapshot().settings);
    off.then(|| error(StatusCode::CONFLICT, optical::OFF))
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({"error": message}))).into_response()
}

fn parse<T: DeserializeOwned>(body: &Bytes) -> Result<T, Response> {
    serde_json::from_slice(body).map_err(|e| error(StatusCode::BAD_REQUEST, &format!("Invalid request: {e}")))
}

fn status(code: u16) -> StatusCode {
    StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
}

fn reply((code, body): (StatusCode, Value)) -> Response {
    (code, Json(body)).into_response()
}

async fn edit(state: &AppState, request: Request) -> (StatusCode, Value) {
    match state.machine.edit(request).await {
        Some(answer) => (status(answer.status), answer.body),
        None => (StatusCode::SERVICE_UNAVAILABLE, json!({"error": "The machine has stopped."})),
    }
}

/// `FRAMES` frames begun after this call, from the capture task's shared camera.
async fn capture(lease: &mut Lease) -> wc_optical::Result<Vec<Frame>> {
    let frames = lease.frames_after(FRAMES, Instant::now(), CAPTURE_WAIT).await?;
    Ok(frames.iter().map(|frame| Frame::clone(frame)).collect())
}

/// The last frame with its square and crosshairs, base64 PNG.
fn annotated(frames: &[Frame], settings: &OpticalSettings) -> wc_optical::Result<String> {
    let frame = frames.last().ok_or(wc_optical::OpticalError::NoFrame)?;
    let square = wc_optical::detect::find_square(frame, &DetectParams::from_settings(settings, Mode::Calibrate)).ok();
    let centre = wc_optical::measure::optical_centre(settings, frame.width, frame.height);
    let png = wc_optical::overlay::annotate(frame, square.as_ref(), centre)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(png))
}

type MeasureFn = fn(&[Frame], &OpticalSettings, Option<&Orientation>) -> wc_optical::Result<Measurement>;
type OrientFn = fn(PixelPoint, PixelPoint, PixelPoint, f64) -> wc_optical::Result<Orientation>;

/// The vision a scan needs; tests stand in for the image maths.
pub(crate) struct Eyes {
    pub measure: MeasureFn,
    pub orientation: OrientFn,
}

impl Default for Eyes {
    fn default() -> Eyes {
        Eyes { measure: wc_optical::measure::measure, orientation: wc_optical::measure::orientation }
    }
}

impl Eyes {
    async fn answer(&self, lease: &mut Lease, request: Vision) -> Seen {
        match request {
            Vision::Measure { id, settings, orientation, .. } => {
                let measure = self.measure;
                let (result, png) = match capture(lease).await {
                    Ok(frames) => tokio::task::spawn_blocking(move || {
                        let measured = measure(&frames, &settings, orientation.as_ref());
                        let png = measured.as_ref().ok().and_then(|_| annotated(&frames, &settings).ok());
                        (measured, png)
                    })
                    .await
                    .expect("vision does not panic"),
                    Err(error) => (Err(error), None),
                };
                Seen::Measured { id, result: result.map_err(|e| e.to_string()), png }
            }
            Vision::Orient { id, start, after_x, after_y, step_mm, .. } => Seen::Oriented {
                id,
                result: (self.orientation)(start, after_x, after_y, step_mm).map_err(|e| e.to_string()),
            },
        }
    }
}

/// Answers scan `id`'s camera requests until that scan ends; the camera closes with the last lease.
pub(crate) async fn serve(
    machine: MachineHandle,
    id: u64,
    mut requests: broadcast::Receiver<Vision>,
    cameras: Cameras,
    eyes: Eyes,
) {
    let (mut lease, started) = (None::<Lease>, Instant::now());
    // The snapshot may lag the scan's start by a step.
    while machine.snapshot().optical.scan == Some(id) || started.elapsed() < POLL {
        let request = match tokio::time::timeout(POLL, requests.recv()).await {
            Ok(Ok(request)) if request.scan() == id => request,
            Ok(Err(broadcast::error::RecvError::Closed)) => break,
            _ => continue,
        };
        let lease = lease.get_or_insert_with(|| cameras.lease(&machine.snapshot().settings));
        machine.send(Event::Seen(eyes.answer(lease, request).await));
    }
}

async fn current(State(state): State<AppState>) -> Response {
    if let Some(refused) = refused(&state) {
        return refused;
    }
    let snapshot = state.machine.snapshot();
    Json(OpticalState {
        settings: optical::settings(&snapshot.settings),
        session: snapshot.optical.session.clone(),
        camera: state.cameras().status(&snapshot.settings),
    })
    .into_response()
}

async fn save(State(state): State<AppState>, body: Bytes) -> Response {
    if let Some(refused) = refused(&state) {
        return refused;
    }
    match parse::<OpticalSettings>(&body) {
        Ok(settings) => reply(edit(&state, Request::OpticalSettings(settings)).await),
        Err(refused) => refused,
    }
}

async fn scan(State(state): State<AppState>, body: Bytes) -> Response {
    if let Some(refused) = refused(&state) {
        return refused;
    }
    let request = match parse::<ScanRequest>(&body) {
        Ok(request) => request,
        Err(refused) => return refused,
    };
    let cells = match request.cell {
        Some(cell) => Ok(vec![cell]),
        None => wc_optical::grid::scan_order(&request.settings),
    };
    let cells = match cells {
        Ok(cells) => cells,
        Err(problem) => return error(StatusCode::BAD_REQUEST, &problem.to_string()),
    };
    let requests = state.machine.vision();
    let (code, mut body) = edit(&state, Request::OpticalScan { settings: request.settings, cells }).await;
    if let Some(id) = body.as_object_mut().and_then(|body| body.remove(optical::SCAN_ID)).and_then(|id| id.as_u64()) {
        tokio::spawn(serve(state.machine.clone(), id, requests, state.cameras().clone(), Eyes::default()));
    }
    reply((code, body))
}

/// Test image and find centre: frames from the camera now, without moving, unless a job or a scan is running.
async fn frames_now(state: &AppState, body: &Bytes) -> Result<(OpticalSettings, Vec<Frame>), Response> {
    let request = parse::<CaptureRequest>(body)?;
    let snapshot = state.machine.snapshot();
    let busy = match () {
        _ if snapshot.optical.scan.is_some() => Some(optical::RUNNING),
        _ if snapshot.job_in_progress || snapshot.manual_z_axis_adjust => Some(optical::JOB_RUNNING),
        _ => None,
    };
    if let Some(busy) = busy {
        return Err(error(StatusCode::CONFLICT, busy));
    }
    let captured = capture(&mut state.cameras().lease(&snapshot.settings)).await;
    captured
        .map(|frames| (request.settings, frames))
        .map_err(|e| error(StatusCode::UNPROCESSABLE_ENTITY, &e.to_string()))
}

async fn test_image(State(state): State<AppState>, body: Bytes) -> Response {
    if let Some(refused) = refused(&state) {
        return refused;
    }
    let (settings, frames) = match frames_now(&state, &body).await {
        Ok(captured) => captured,
        Err(refused) => return refused,
    };
    match annotated(&frames, &settings) {
        Ok(png) => {
            let measurement = wc_optical::measure::measure(&frames, &settings, None).ok();
            Json(TestImage { png, measurement }).into_response()
        }
        Err(problem) => error(StatusCode::UNPROCESSABLE_ENTITY, &problem.to_string()),
    }
}

async fn find_center(State(state): State<AppState>, body: Bytes) -> Response {
    if let Some(refused) = refused(&state) {
        return refused;
    }
    let (settings, frames) = match frames_now(&state, &body).await {
        Ok(captured) => captured,
        Err(refused) => return refused,
    };
    match wc_optical::measure::find_centre(&frames, &settings) {
        Ok(centre) => Json(CenterFound { optical_center_x: centre.x, optical_center_y: centre.y }).into_response(),
        Err(problem) => error(StatusCode::UNPROCESSABLE_ENTITY, &problem.to_string()),
    }
}

async fn fit(State(state): State<AppState>) -> Response {
    if let Some(refused) = refused(&state) {
        return refused;
    }
    let snapshot = state.machine.snapshot();
    if snapshot.optical.scan.is_some() {
        return error(StatusCode::CONFLICT, optical::RUNNING);
    }
    let (settings, measured) = (snapshot.settings.0.clone(), snapshot.optical.grid.measured());
    let fitted = tokio::task::spawn_blocking(move || optical::fit(&settings, &measured)).await;
    match fitted.expect("the fit does not panic") {
        Ok(fit) => reply(edit(&state, Request::OpticalFitted(Box::new(fit))).await),
        Err((code, message)) => error(status(code), &message),
    }
}

async fn accept(State(state): State<AppState>) -> Response {
    if let Some(refused) = refused(&state) {
        return refused;
    }
    reply(edit(&state, Request::OpticalAccept).await)
}

fn download(content_type: &str, name: &str, body: Vec<u8>) -> Response {
    let disposition = format!("attachment; filename=\"{name}\"");
    ([(header::CONTENT_TYPE, content_type.to_string()), (header::CONTENT_DISPOSITION, disposition)], body)
        .into_response()
}

async fn csv(State(state): State<AppState>) -> Response {
    if let Some(refused) = refused(&state) {
        return refused;
    }
    let grid = state.machine.snapshot().optical.grid.clone();
    if grid.measured().is_empty() {
        return error(StatusCode::CONFLICT, optical::NOTHING_MEASURED);
    }
    match wc_optical::codec::csv(&grid) {
        Ok(text) => download("text/csv", "calibrationValues.csv", text.into_bytes()),
        Err(problem) => error(StatusCode::INTERNAL_SERVER_ERROR, &problem.to_string()),
    }
}

async fn target(State(state): State<AppState>, Query(query): Query<TargetQuery>) -> Response {
    if let Some(refused) = refused(&state) {
        return refused;
    }
    let settings = optical::settings(&state.machine.snapshot().settings);
    let name = format!("optical-target-{}.pdf", if query.paper == wc_optical::Paper::A4 { "a4" } else { "letter" });
    match wc_optical::target::pdf(&wc_optical::render::Target::nominal(&settings), query.paper) {
        Ok(pdf) => download("application/pdf", &name, pdf),
        Err(problem) => error(StatusCode::INTERNAL_SERVER_ERROR, &problem.to_string()),
    }
}

#[cfg(test)]
mod tests {
    //! The scan's camera server with a stand-in camera and image maths, against the simulated controller.

    use std::sync::{Arc, Mutex};

    use wc_config::Settings;
    use wc_machine::{Machine, NoHost, Snapshot, spawn};
    use wc_optical::OpticalError;
    use wc_optical::camera::{Camera, CameraConfig, SledPosition, Source};
    use wc_protocol::main::LinkStatus;
    use wc_protocol::optical::{Cell, ScanState};

    use super::*;
    use crate::camera::Opener;
    use crate::serial::SimOpener;

    /// Each frame carries the simulated sled's true X and Y; `running` tells whether it is started.
    struct SledCamera(SledPosition, Arc<Mutex<bool>>);

    impl Camera for SledCamera {
        fn start(&mut self, _: &CameraConfig) -> wc_optical::Result<()> {
            *self.1.lock().unwrap() = true;
            Ok(())
        }
        fn next_frame(&mut self, _: Instant) -> wc_optical::Result<Frame> {
            let (x, y) = (self.0)();
            Ok(Frame { width: 16, height: 1, luma: [x.to_le_bytes(), y.to_le_bytes()].concat() })
        }
        fn stop(&mut self) {
            *self.1.lock().unwrap() = false;
        }
    }

    /// The target's squares sit 0.5 mm right of and 0.3 mm below their nominal places.
    fn seen(frames: &[Frame], _: &OpticalSettings, _: Option<&Orientation>) -> wc_optical::Result<Measurement> {
        let luma = &frames.last().ok_or(OpticalError::NoFrame)?.luma;
        let read = |at: usize| f64::from_le_bytes(luma[at..at + 8].try_into().unwrap());
        let square = |sled: f64, offset: f64| ((sled - offset) / 76.2).round() * 76.2 + offset - sled;
        let (dx, dy) = (square(read(0), 0.5), square(read(8), -0.3));
        let centre_px = PixelPoint { x: 320.0 + dx * 10.0, y: 240.0 - dy * 10.0 };
        Ok(Measurement { dx_mm: dx, dy_mm: dy, centre_px, angle_deg: 0.0, frames_used: frames.len() as u32 })
    }

    fn upright(_: PixelPoint, _: PixelPoint, _: PixelPoint, _: f64) -> wc_optical::Result<Orientation> {
        Ok(Orientation { rotation_deg: 0.0, mirrored: false, px_per_mm: 10.0 })
    }

    fn machine() -> (MachineHandle, SledPosition) {
        let opener = SimOpener::new(wc_sim::Build::holey_51_27());
        let sled = crate::cli::sled_position(opener.sim());
        let mut settings = Settings::defaults();
        settings.set_value(optical::ENABLED_SECTION, optical::ENABLED_KEY, json!(1));
        settings.set_value("Maslow Settings", "zAxis", json!(1));
        settings.set_value("Camera Settings", "fps", json!(50));
        let machine = Machine::new("h", settings).on_port("simulator");
        (spawn(machine, Box::new(opener), Arc::new(NoHost)).0, sled)
    }

    /// Cameras whose opener counts opens and whose camera reports whether it runs.
    fn cameras(sled: SledPosition) -> (Cameras, Arc<Mutex<u32>>, Arc<Mutex<bool>>) {
        let (opened, running) = (Arc::new(Mutex::new(0)), Arc::new(Mutex::new(false)));
        let (count, started) = (opened.clone(), running.clone());
        let opener: Opener = Arc::new(move |_: &Source, _: Option<SledPosition>| {
            *count.lock().unwrap() += 1;
            Ok(Box::new(SledCamera(sled.clone(), started.clone())) as Box<dyn Camera>)
        });
        (Cameras::with_opener(None, opener), opened, running)
    }

    async fn until(machine: &MachineHandle, done: impl Fn(&Snapshot) -> bool) {
        let waited = async {
            while !done(&machine.snapshot()) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        };
        let late = tokio::time::timeout(Duration::from_secs(20), waited).await.is_err();
        let snapshot = machine.snapshot();
        assert!(!late, "{:?} {:?}", snapshot.link, snapshot.optical.session.as_ref().map(|s| s.state));
    }

    async fn scan(machine: &MachineHandle, cameras: Cameras, eyes: Eyes) {
        until(machine, |s| s.link == LinkStatus::Connected).await;
        // The boot handshake and its settings sync go first.
        tokio::time::sleep(Duration::from_millis(1500)).await;
        let settings = OpticalSettings { settle_time: 0.0, ..optical::settings(&Settings::defaults()) };
        let cells = vec![Cell { i: 0, j: 0 }, Cell { i: 1, j: 1 }];
        let requests = machine.vision();
        let answer = machine.edit(Request::OpticalScan { settings, cells }).await.unwrap();
        assert_eq!(answer.status, 200, "{}", answer.body);
        let id = answer.body[optical::SCAN_ID].as_u64().unwrap();
        tokio::spawn(serve(machine.clone(), id, requests, cameras, eyes));
    }

    fn state(snapshot: &Snapshot) -> Option<ScanState> {
        snapshot.optical.session.as_ref().map(|session| session.state)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_camera_server_answers_a_whole_scan_then_lets_the_camera_go() {
        let (machine, sled) = machine();
        let (cameras, opened, running) = cameras(sled);
        scan(&machine, cameras.clone(), Eyes { measure: seen, orientation: upright }).await;
        until(&machine, |s| state(s) == Some(ScanState::Done)).await;
        let grid = machine.snapshot().optical.grid.clone();
        let (x, y) = grid.get(Cell { i: 1, j: 1 }).expect("measured");
        assert!((x - 0.5).abs() < 1e-6 && (y + 0.3).abs() < 1e-6, "{x}, {y}");
        assert_eq!(*opened.lock().unwrap(), 1, "the camera opened once and stayed open");
        let stopped = async {
            while *running.lock().unwrap() || cameras.running() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        };
        tokio::time::timeout(Duration::from_secs(2), stopped).await.expect("the camera stops with the scan");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_scan_shares_the_open_camera_with_the_live_overlay() {
        let (machine, sled) = machine();
        let (cameras, opened, running) = cameras(sled);
        let overlay = cameras.lease(&machine.snapshot().settings);
        scan(&machine, cameras.clone(), Eyes { measure: seen, orientation: upright }).await;
        until(&machine, |s| state(s) == Some(ScanState::Done)).await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert_eq!(*opened.lock().unwrap(), 1, "one camera serves both");
        assert!(*running.lock().unwrap(), "the overlay still holds it");
        drop(overlay);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_camera_that_will_not_open_fails_the_square_and_stop_ends_the_server() {
        let (machine, _) = machine();
        let opener: Opener =
            Arc::new(|_: &Source, _: Option<SledPosition>| Err(OpticalError::Camera("unplugged".into())));
        let cameras = Cameras::with_opener(None, opener);
        scan(&machine, cameras, Eyes { measure: seen, orientation: upright }).await;
        until(&machine, |s| state(s) == Some(ScanState::Failed)).await;
        machine.send(Event::Action(wc_protocol::Action::new("stopRun", json!(""), json!(""))));
        until(&machine, |s| state(s) == Some(ScanState::Stopped)).await;
        assert!(machine.snapshot().optical.scan.is_none());
    }

    fn slow(frames: &[Frame], settings: &OpticalSettings, at: Option<&Orientation>) -> wc_optical::Result<Measurement> {
        std::thread::sleep(Duration::from_secs(3));
        seen(frames, settings, at)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn stop_does_not_wait_for_a_measurement() {
        let (machine, sled) = machine();
        scan(&machine, cameras(sled).0, Eyes { measure: slow, orientation: upright }).await;
        until(&machine, |s| state(s) == Some(ScanState::Measuring)).await;
        let pressed = Instant::now();
        machine.send(Event::Action(wc_protocol::Action::new("stopRun", json!(""), json!(""))));
        until(&machine, |s| state(s) == Some(ScanState::Stopped)).await;
        assert!(pressed.elapsed() < Duration::from_millis(100), "{:?}", pressed.elapsed());
    }

    #[test]
    fn annotating_needs_a_frame() {
        let settings = optical::settings(&Settings::defaults());
        assert_eq!(annotated(&[], &settings), Err(OpticalError::NoFrame));
    }
}
