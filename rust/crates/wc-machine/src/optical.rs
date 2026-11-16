//! Experimental optical calibration (`spec/optical-calibration.md`): the setting gate, the scan's state machine,
//! the session and Accept. Capture and vision run off the machine task: it asks with [`Vision`] and hears [`Seen`].

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use wc_calibration::CalibrationError;
use wc_calibration::holey::HoleyResult;
use wc_config::{Settings, python_float_str};
use wc_optical::grid::{self, ErrorGrid};
use wc_protocol::float;
use wc_protocol::optical::{
    Cell, GeometryFit, Measurement, OpticalSettings, Orientation, PixelPoint, ScanImage, ScanProgress, ScanStarted,
    ScanState, Session, SettingsSaved,
};

use crate::clearance::Raise;
use crate::edits::Response;
use crate::machine::{Effect, Link, Machine, alert, broadcast};
use crate::setup::Measure;

pub const ENABLED_SECTION: &str = "WebControl Settings";
pub const ENABLED_KEY: &str = "opticalCalibrationEnabled";
/// Its name in `GET /api/state`'s `experimental`.
pub const FEATURE: &str = "optical";
pub const OFF: &str = "Optical calibration is turned off. Turn on \"Optical calibration (experimental)\" in Settings > WebControl Settings.";
pub const SECTION: &str = "Optical Calibration Settings";
pub const RUNNING: &str = "Optical calibration is running; press Stop first";
pub const NOTHING_MEASURED: &str = "Nothing has been measured yet. Scan the target first.";
pub const NOTHING_TO_ACCEPT: &str = "There is no fitted geometry to accept. Scan the target and press Fit first.";
pub const JOB_RUNNING: &str = "Stop the job before running optical calibration.";
pub const NOT_CONNECTED: &str = "The controller is not connected.";
pub const NO_SAFE_HEIGHT: &str = "Set the Z-axis safe height to a number, so the bit rises before the scan moves.";
pub const ATTEMPTS: u32 = 6;
pub const MAX_STEP_MM: f64 = 10.0;
pub const MAX_CORRECTION_MM: f64 = 25.0;
/// The orientation check's move in +X, then in +Y.
pub const ORIENT_STEP_MM: f64 = 5.0;
pub const MAX_TILT_DEG: f64 = 15.0;
pub const SCALE_TOLERANCE: f64 = 0.05;
/// How long the camera may take to capture and measure one square.
pub const VISION_WAIT: Duration = Duration::from_secs(30);
/// Allowed beyond a move's own time at the feed rate before its `B10` reply counts as lost.
pub const MOVE_SLACK: Duration = Duration::from_secs(15);
/// The first move also waits for the raise.
const RAISE_SLACK: Duration = Duration::from_secs(60);
/// The started scan's id in the machine's answer, which the route keeps and does not send.
pub const SCAN_ID: &str = "scan";
pub(crate) const COMMANDS: [&str; 3] = ["opticalRetryCell", "opticalSkipCell", "opticalClearSession"];

/// Read on every request, so the setting takes effect without a restart.
pub fn enabled(settings: &Settings) -> bool {
    matches!(settings.value(ENABLED_SECTION, ENABLED_KEY), Some(v) if v == 1 || v == true)
}

pub fn experimental(settings: &Settings) -> Vec<String> {
    enabled(settings).then(|| FEATURE.to_string()).into_iter().collect()
}

/// The stored "Optical Calibration Settings".
pub fn settings(settings: &Settings) -> OpticalSettings {
    let number = |key: &str| settings.number(SECTION, key);
    let int = |key: &str| number(key) as i32;
    OpticalSettings {
        marker_x: number("markerX"),
        marker_y: number("markerY"),
        scale_x: number("scaleX"),
        scale_y: number("scaleY"),
        optical_center_x: number("opticalCenterX"),
        optical_center_y: number("opticalCenterY"),
        position_tolerance: number("positionTolerance"),
        tl_x: int("tlX"),
        tl_y: int("tlY"),
        br_x: int("brX"),
        br_y: int("brY"),
        calibration_extents: settings.text_or_empty(SECTION, "calibrationExtents"),
        auto_scan_direction: int("autoScanDirection"),
        gaussian_blur_value: int("gaussianBlurValue"),
        canny_low_value: number("cannyLowValue"),
        canny_high_value: number("cannyHighValue"),
        feed_rate: number("feedRate"),
        settle_time: number("settleTime"),
    }
}

/// Why settings cannot be saved or scanned with.
pub fn check(settings: &OpticalSettings) -> Result<(), String> {
    let s = settings;
    let positive = [("markerX", s.marker_x), ("markerY", s.marker_y), ("scaleX", s.scale_x), ("scaleY", s.scale_y)];
    let positive = positive.into_iter().chain([("positionTolerance", s.position_tolerance), ("feedRate", s.feed_rate)]);
    let others = [s.optical_center_x, s.optical_center_y, s.canny_low_value, s.canny_high_value, s.settle_time];
    match { positive }.find(|(_, v)| !(v.is_finite() && *v > 0.0)) {
        Some((key, _)) => Err(format!("{key} must be a number above 0.")),
        None if !others.iter().all(|v| v.is_finite()) || s.settle_time < 0.0 => {
            Err("Every optical setting must be a number, and settleTime 0 or more.".into())
        }
        None => Ok(()),
    }
}

/// `wc_calibration::optical_fit::fit` of the measured squares, or the status and message the route answers.
pub fn fit(settings: &Settings, measured: &[(Cell, (f64, f64))]) -> Result<GeometryFit, (u16, String)> {
    if measured.is_empty() {
        return Err((409, NOTHING_MEASURED.into()));
    }
    wc_calibration::optical_fit::fit(settings, measured).map_err(|error| match error {
        CalibrationError::NoSolution | CalibrationError::NotFinite => (422, error.to_string()),
        other => (409, other.to_string()),
    })
}

/// Asked of the camera, off the machine task; answered by `Event::Seen` with the same `id`.
#[derive(Debug, Clone, PartialEq)]
pub enum Vision {
    /// The square under the camera, from frames captured after the request. `target_mm` is where the sled was sent.
    Measure {
        scan: u64,
        id: u64,
        cell: Cell,
        target_mm: (f64, f64),
        settings: OpticalSettings,
        orientation: Option<Orientation>,
    },
    /// `wc_optical::measure::orientation` of three pixel centres, `step_mm` apart in +X, then +Y.
    Orient { scan: u64, id: u64, start: PixelPoint, after_x: PixelPoint, after_y: PixelPoint, step_mm: f64 },
}

impl Vision {
    pub fn scan(&self) -> u64 {
        match self {
            Vision::Measure { scan, .. } | Vision::Orient { scan, .. } => *scan,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Seen {
    /// `png` is the annotated frame, base64.
    Measured {
        id: u64,
        result: Result<Measurement, String>,
        png: Option<String>,
    },
    Oriented {
        id: u64,
        result: Result<Orientation, String>,
    },
}

/// What readers see of optical calibration.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct View {
    pub session: Option<Session>,
    pub grid: ErrorGrid,
    /// The running scan's id, from the scan's start until it ends (a failed square still holds it).
    pub scan: Option<u64>,
    pub fit: Option<GeometryFit>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Wait {
    Move { until: Duration },
    Settle { until: Duration },
    Vision { id: u64, until: Duration },
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
struct Scan {
    id: u64,
    cells: Vec<Cell>,
    at: usize,
    attempt: u32,
    /// The correction tried at this square; the next square starts from it.
    guess: (f64, f64),
    /// The correction at this square's first attempt, which Retry goes back to.
    first_guess: (f64, f64),
    /// While the orientation check runs: the squares measured so far.
    orienting: Option<Vec<Measurement>>,
    wait: Wait,
    target: Option<(f64, f64)>,
    restore_inches: bool,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Optical {
    scan: Option<Scan>,
    state: Option<ScanState>,
    settings: Option<OpticalSettings>,
    grid: ErrorGrid,
    orientation: Option<Orientation>,
    fit: Option<GeometryFit>,
    next_id: u64,
    view: Arc<View>,
}

impl Optical {
    pub fn view(&self) -> Arc<View> {
        self.view.clone()
    }

    pub fn holds(&self) -> bool {
        self.scan.is_some()
    }

    fn refresh(&mut self) {
        let (error_x, error_y) = self.grid.columns();
        let session = self.settings.clone().map(|settings| Session {
            state: self.state.unwrap_or(ScanState::Idle),
            settings,
            error_x,
            error_y,
            orientation: self.orientation,
        });
        let view =
            View { session, grid: self.grid.clone(), scan: self.scan.as_ref().map(|s| s.id), fit: self.fit.clone() };
        if *self.view != view {
            self.view = Arc::new(view);
        }
    }
}

fn hypot((x, y): (f64, f64)) -> f64 {
    x.hypot(y)
}

/// Moves go out to the micron, so the sled is where the rounded target says.
fn micron(mm: f64) -> f64 {
    (mm * 1000.0).round() / 1000.0
}

impl Machine {
    pub(crate) fn optical_holds(&self) -> bool {
        self.optical.holds()
    }

    fn optical_refusal(&self) -> Option<&'static str> {
        match () {
            _ if self.optical.holds() => Some(RUNNING),
            _ if self.gcode_locked() => Some(JOB_RUNNING),
            _ if self.probing => Some(crate::actions::PROBING),
            _ if self.alarm_held() => Some(crate::alarm::HELD),
            _ if self.upgrade.holds_link() => Some("Wait for the firmware upgrade to finish."),
            _ => None,
        }
    }

    /// `POST /api/calibration/optical/scan`, with the squares in order. One square keeps the rest of the session.
    pub(crate) fn optical_scan(
        &mut self,
        settings: OpticalSettings,
        cells: Vec<Cell>,
        now: Duration,
        effects: &mut Vec<Effect>,
    ) -> Response {
        if let Some(refusal) = self.optical_refusal() {
            return Response::error(409, refusal);
        }
        if !matches!(self.link, Link::Connected { .. }) {
            return Response::error(409, NOT_CONNECTED);
        }
        let Some(safe_height_mm) = self.settings.value("Maslow Settings", "zAxisSafeHeight").and_then(float) else {
            return Response::error(409, NO_SAFE_HEIGHT);
        };
        let (half_width, half_height) = (self.bed("bedWidth") / 2.0, self.bed("bedHeight") / 2.0);
        let outside = cells.iter().find(|cell| {
            let (x, y) = grid::nominal_mm(**cell);
            grid::index(**cell).is_none() || x.abs() > half_width || y.abs() > half_height
        });
        let problem = match (outside, check(&settings)) {
            (_, Err(problem)) => Some(problem),
            (Some(cell), _) => Some(format!("Square ({}, {}) is outside the work area.", cell.i, cell.j)),
            _ if cells.is_empty() => Some("The extents hold no squares.".into()),
            _ => None,
        };
        if let Some(problem) = problem {
            return Response::error(400, &problem);
        }
        let restore_inches = self.units() == wc_protocol::main::Units::Inches;
        let optical = &mut self.optical;
        if cells.len() > 1 || optical.settings.is_none() {
            optical.grid = ErrorGrid::new();
            optical.orientation = None;
        }
        optical.fit = None;
        optical.settings = Some(settings);
        optical.next_id += 1;
        let total = cells.len() as u32;
        optical.scan = Some(Scan {
            id: optical.next_id,
            cells,
            at: 0,
            attempt: 1,
            guess: (0.0, 0.0),
            first_guess: (0.0, 0.0),
            orienting: Some(Vec::new()),
            wait: Wait::Failed,
            target: None,
            restore_inches,
        });
        self.queue(["G21 ".to_string(), "G90 ".to_string()]);
        self.queue_raise(Raise::Accept { safe_height_mm });
        self.optical_move(now, effects);
        let mut body = serde_json::to_value(ScanStarted { total }).unwrap_or(Value::Null);
        body[SCAN_ID] = Value::from(self.optical.scan.as_ref().map(|scan| scan.id));
        Response { status: 200, body }
    }

    fn bed(&self, key: &str) -> f64 {
        self.settings.number("Maslow Settings", key)
    }

    /// Sends the sled to the current square (or orientation step), then `B10 L`, whose reply means it arrived.
    fn optical_move(&mut self, now: Duration, effects: &mut Vec<Effect>) {
        let feed = self.optical.settings.as_ref().map_or(0.0, |s| s.feed_rate);
        let Some(scan) = self.optical.scan.as_mut() else { return };
        let (x, y) = grid::nominal_mm(scan.cells[scan.at]);
        let step = match scan.orienting.as_ref().map(Vec::len) {
            Some(1) => (ORIENT_STEP_MM, 0.0),
            Some(2) => (0.0, ORIENT_STEP_MM),
            _ => (0.0, 0.0),
        };
        let target = (micron(x + scan.guess.0 + step.0), micron(y + scan.guess.1 + step.1));
        let travel = scan.target.map(|from| hypot((target.0 - from.0, target.1 - from.1)));
        let slack = if travel.is_some() { MOVE_SLACK } else { RAISE_SLACK };
        let travel_s = travel.unwrap_or_else(|| hypot((x.abs() * 2.0, y.abs() * 2.0)) + 1.0) / feed * 60.0;
        scan.wait = Wait::Move { until: now + slack + Duration::from_secs_f64(travel_s.min(3600.0)) };
        scan.target = Some(target);
        let line =
            format!("G1 X{} Y{} F{}", python_float_str(target.0), python_float_str(target.1), python_float_str(feed));
        self.queue([line, "B10 L".to_string()]);
        self.measure = Some(Measure::Optical);
        self.optical_progress(None, None, effects);
    }

    /// The `B10` reply: every line before it has run, so the sled is there; it settles before the capture.
    pub(crate) fn optical_arrived(&mut self, now: Duration, effects: &mut Vec<Effect>) {
        let settle = self.optical.settings.as_ref().map_or(0.0, |s| s.settle_time);
        let Some(scan) = self.optical.scan.as_mut().filter(|s| matches!(s.wait, Wait::Move { .. })) else { return };
        scan.wait = Wait::Settle { until: now + Duration::from_secs_f64(settle) };
        self.optical_progress(None, None, effects);
    }

    /// Every event: settling ends in a capture; a lost link, a lost `B10` reply or a silent camera ends the wait.
    pub(crate) fn optical_tick(&mut self, now: Duration, effects: &mut Vec<Effect>) {
        let Some(wait) = self.optical.scan.as_ref().map(|scan| scan.wait) else { return };
        if !matches!(self.link, Link::Connected { .. }) {
            return self.optical_finish(
                ScanState::Stopped,
                Some("The controller's link was lost, so the scan stopped."),
                effects,
            );
        }
        match wait {
            Wait::Settle { until } if now >= until => self.optical_capture(now, effects),
            Wait::Move { until } if now >= until => {
                self.measure = None;
                self.optical_fail("The controller did not report the move done.".into(), effects);
            }
            Wait::Vision { until, .. } if now >= until => {
                self.optical_fail("The camera did not answer.".into(), effects)
            }
            _ => {}
        }
    }

    fn optical_capture(&mut self, now: Duration, effects: &mut Vec<Effect>) {
        let optical = &mut self.optical;
        let (Some(scan), Some(settings)) = (optical.scan.as_mut(), optical.settings.clone()) else { return };
        optical.next_id += 1;
        let id = optical.next_id;
        scan.wait = Wait::Vision { id, until: now + VISION_WAIT };
        let orientation = if scan.orienting.is_some() { None } else { optical.orientation };
        let (cell, target_mm) = (scan.cells[scan.at], scan.target.unwrap_or_default());
        effects.push(Effect::Vision(Vision::Measure { scan: scan.id, id, cell, target_mm, settings, orientation }));
        self.optical_progress(None, None, effects);
    }

    /// The camera's answer, if it is the one the scan waits for.
    pub(crate) fn optical_seen(&mut self, seen: Seen, now: Duration, effects: &mut Vec<Effect>) {
        let waiting = self.optical.scan.as_ref().and_then(|scan| match scan.wait {
            Wait::Vision { id, .. } => Some((id, scan.cells[scan.at])),
            _ => None,
        });
        match seen {
            Seen::Measured { id, result, png } if waiting.is_some_and(|(wanted, _)| wanted == id) => {
                if let (Some(png), Some((_, cell))) = (png, waiting) {
                    effects.push(broadcast(&ScanImage { cell, png }));
                }
                match result {
                    Ok(measurement) => self.optical_measured(measurement, now, effects),
                    Err(error) => self.optical_fail(error, effects),
                }
            }
            Seen::Oriented { id, result } if waiting.is_some_and(|(wanted, _)| wanted == id) => match result {
                Ok(orientation) => self.optical_oriented(orientation, now, effects),
                Err(error) => self.optical_fail(error, effects),
            },
            _ => tracing::debug!("a camera answer nobody waits for"),
        }
    }

    fn optical_measured(&mut self, measurement: Measurement, now: Duration, effects: &mut Vec<Effect>) {
        let Some(scan) = self.optical.scan.as_mut() else { return };
        if let Some(seen) = scan.orienting.as_mut() {
            seen.push(measurement);
            if seen.len() < 3 {
                return self.optical_move(now, effects);
            }
            let (moved_x, moved_y) = (shift(&seen[0], &seen[1]), shift(&seen[0], &seen[2]));
            let off = |moved: f64| (moved / ORIENT_STEP_MM - 1.0).abs() > SCALE_TOLERANCE;
            if off(moved_x) || off(moved_y) {
                let why = format!(
                    "The camera saw the square move {:.2} mm and {:.2} mm for {ORIENT_STEP_MM} mm moves. Check the \
                     marker size and the target's scale.",
                    moved_x, moved_y
                );
                return self.optical_fail(why, effects);
            }
            self.optical.next_id += 1;
            let id = self.optical.next_id;
            let (start, after_x, after_y) = (seen[0].centre_px, seen[1].centre_px, seen[2].centre_px);
            let scan_id = scan.id;
            scan.wait = Wait::Vision { id, until: now + VISION_WAIT };
            let step_mm = ORIENT_STEP_MM;
            return effects.push(Effect::Vision(Vision::Orient {
                scan: scan_id,
                id,
                start,
                after_x,
                after_y,
                step_mm,
            }));
        }
        let d = (measurement.dx_mm, measurement.dy_mm);
        let tolerance = self.optical.settings.as_ref().map_or(0.0, |s| s.position_tolerance);
        let (at, nominal) = (scan.target.unwrap_or_default(), grid::nominal_mm(scan.cells[scan.at]));
        let guess = (at.0 - nominal.0 + d.0, at.1 - nominal.1 + d.1);
        let problem = match () {
            _ if hypot(d) > MAX_STEP_MM => {
                Some(format!("The square was {:.1} mm off, more than one {MAX_STEP_MM} mm step.", hypot(d)))
            }
            _ if hypot(guess) > MAX_CORRECTION_MM => Some(format!(
                "The square needs {:.1} mm of correction, more than {MAX_CORRECTION_MM} mm.",
                hypot(guess)
            )),
            _ => None,
        };
        if let Some(problem) = problem {
            return self.optical_fail(problem, effects);
        }
        scan.guess = guess;
        if d.0.abs() < tolerance && d.1.abs() < tolerance {
            let cell = scan.cells[scan.at];
            let scale = self.optical.settings.as_ref().map_or((1.0, 1.0), |s| (s.scale_x, s.scale_y));
            let (x, y) = grid::nominal_mm(cell);
            self.optical.grid.set(cell, Some((guess.0 + x * (1.0 - scale.0), guess.1 + y * (1.0 - scale.1))));
            self.optical_progress(Some(measurement), None, effects);
            return self.optical_advance(now, effects);
        }
        if scan.attempt >= ATTEMPTS {
            return self.optical_fail(format!("The square was not centred within {ATTEMPTS} attempts."), effects);
        }
        scan.attempt += 1;
        self.optical_move(now, effects);
    }

    fn optical_oriented(&mut self, orientation: Orientation, now: Duration, effects: &mut Vec<Effect>) {
        let problem = match orientation {
            o if o.mirrored => {
                Some("The camera's image is mirrored. Mount it so the image is not flipped.".to_string())
            }
            o if o.rotation_deg.abs() > MAX_TILT_DEG => Some(format!(
                "The camera is turned {:.1}° from the machine's axes, more than {MAX_TILT_DEG}°. Mount it square to \
                 the sled.",
                o.rotation_deg
            )),
            _ => None,
        };
        if let Some(problem) = problem {
            return self.optical_fail(problem, effects);
        }
        self.optical.orientation = Some(orientation);
        effects.push(broadcast(&orientation));
        if let Some(scan) = self.optical.scan.as_mut() {
            scan.orienting = None;
        }
        self.optical_move(now, effects);
    }

    fn optical_advance(&mut self, now: Duration, effects: &mut Vec<Effect>) {
        let Some(scan) = self.optical.scan.as_mut() else { return };
        scan.at += 1;
        scan.attempt = 1;
        scan.first_guess = scan.guess;
        match scan.at < scan.cells.len() {
            true => self.optical_move(now, effects),
            false => self.optical_finish(ScanState::Done, None, effects),
        }
    }

    /// A square that failed pauses the scan; Retry, Skip or Stop carries on.
    fn optical_fail(&mut self, error: String, effects: &mut Vec<Effect>) {
        let Some(scan) = self.optical.scan.as_mut() else { return };
        scan.wait = Wait::Failed;
        effects.push(alert(&format!("{error} Press Retry, Skip or Stop.")));
        self.optical_progress(None, Some(error), effects);
    }

    /// The scan ends: the positioning mode and units it found come back. The grid stays for review.
    fn optical_finish(&mut self, state: ScanState, error: Option<&str>, effects: &mut Vec<Effect>) {
        let Some(scan) = self.optical.scan.take() else { return };
        if self.measure == Some(Measure::Optical) {
            self.measure = None;
        }
        let restore = scan.restore_inches.then(|| "G20 ".to_string());
        self.queue(restore.into_iter().chain(["G90 ".to_string()]));
        self.optical.state = Some(state);
        let done = scan.at.min(scan.cells.len()) as u32;
        let progress = ScanProgress {
            state,
            cell: None,
            attempt: 0,
            done,
            total: scan.cells.len() as u32,
            measurement: None,
            error: error.map(String::from),
        };
        effects.push(broadcast(&progress));
    }

    /// Stop, or an alarm, ends a scan at once.
    pub(crate) fn optical_stopped(&mut self, why: Option<&str>, effects: &mut Vec<Effect>) {
        self.optical_finish(ScanState::Stopped, why, effects);
    }

    fn optical_progress(&mut self, measurement: Option<Measurement>, error: Option<String>, effects: &mut Vec<Effect>) {
        let Some(scan) = self.optical.scan.as_ref() else { return };
        let state = match scan.wait {
            _ if scan.orienting.is_some() && scan.wait != Wait::Failed => ScanState::Orienting,
            Wait::Move { .. } => ScanState::Moving,
            Wait::Settle { .. } => ScanState::Settling,
            Wait::Vision { .. } => ScanState::Measuring,
            Wait::Failed => ScanState::Failed,
        };
        self.optical.state = Some(state);
        let (cell, attempt, done, total) = (scan.cells[scan.at], scan.attempt, scan.at as u32, scan.cells.len() as u32);
        effects.push(broadcast(&ScanProgress { state, cell: Some(cell), attempt, done, total, measurement, error }));
    }

    pub(crate) fn optical_action(&mut self, command: &str, now: Duration, effects: &mut Vec<Effect>) -> Option<String> {
        if !enabled(&self.settings) {
            return Some(OFF.into());
        }
        let failed = self.optical.scan.as_ref().is_some_and(|scan| scan.wait == Wait::Failed);
        match command {
            "opticalClearSession" if self.optical.holds() => Some(RUNNING.into()),
            "opticalClearSession" => {
                self.optical =
                    Optical { next_id: self.optical.next_id, view: self.optical.view(), ..Optical::default() };
                None
            }
            _ if !failed => Some("No square is waiting: the scan has not stopped on a failed square.".into()),
            "opticalSkipCell" if self.optical.scan.as_ref().is_some_and(|s| s.orienting.is_some()) => {
                Some("The camera check cannot be skipped. Press Retry or Stop.".into())
            }
            "opticalSkipCell" => {
                self.optical_advance(now, effects);
                None
            }
            _ => {
                let scan = self.optical.scan.as_mut()?;
                scan.attempt = 1;
                scan.guess = scan.first_guess;
                scan.orienting = scan.orienting.as_ref().map(|_| Vec::new());
                self.optical_move(now, effects);
                None
            }
        }
    }

    /// Settings, sends and imports would change the machine under the scan.
    pub(crate) fn optical_edit_refused(&self) -> Option<Response> {
        self.optical.holds().then(|| Response::error(409, RUNNING))
    }

    pub(crate) fn optical_save(&mut self, settings: OpticalSettings) -> Response {
        if self.gcode_locked() {
            return Response::error(409, crate::files::SETTINGS_LOCKED);
        }
        if let Err(problem) = check(&settings) {
            return Response::error(400, &problem);
        }
        let values = serde_json::to_value(&settings).unwrap_or(Value::Null);
        values.as_object().into_iter().flatten().for_each(|(key, value)| {
            self.write_setting(SECTION, key, value);
        });
        let saved = SettingsSaved { settings: self::settings(&self.settings) };
        Response { status: 200, body: serde_json::to_value(saved).unwrap_or(Value::Null) }
    }

    /// The fit the route computed; Accept may store it until the next scan.
    pub(crate) fn optical_fitted(&mut self, fit: GeometryFit) -> Response {
        if let Some(refusal) = self.optical.holds().then_some(RUNNING) {
            return Response::error(409, refusal);
        }
        let body = serde_json::to_value(&fit).unwrap_or(Value::Null);
        self.optical.fit = Some(fit);
        Response { status: 200, body }
    }

    /// Stores the last fit through the exact `$` writes holey calibration's Accept makes, then forgets it.
    pub(crate) fn optical_accept(&mut self) -> Response {
        if let Some(refusal) = self.optical_refusal() {
            return Response::error(409, refusal);
        }
        let Some(fit) = self.optical.fit.take() else {
            return Response::error(409, NOTHING_TO_ACCEPT);
        };
        let fitted = fit.fitted;
        let accept = HoleyResult {
            motor_y_offset: fitted.motor_yoffset,
            distance_between_motors: fitted.distance_between_motors,
            left_chain_tolerance: fitted.left_chain_tolerance,
            right_chain_tolerance: fitted.right_chain_tolerance,
            total_error: fit.rms_mm,
        }
        .accept();
        if let Err(error) = accept.check() {
            return Response::error(409, &error.to_string());
        }
        accept.writes.iter().for_each(|write| {
            self.write_setting(write.section, write.key, &Value::from(write.value.clone()));
        });
        let parameters = crate::calibration::holey_parameters(&self.settings);
        Response { status: 200, body: serde_json::to_value(parameters).unwrap_or(Value::Null) }
    }

    pub(crate) fn optical_refresh(&mut self) {
        self.optical.refresh();
    }
}

/// Sends and imports, which would write to the controller or the settings under the scan.
pub(crate) fn refuses(request: &crate::files::Request) -> bool {
    use crate::files::Request::{ImportIni, ImportWebControl, RestoreSettings, SendGcode};
    matches!(request, SendGcode(_) | ImportIni(_) | ImportWebControl(_) | RestoreSettings(_))
}

/// How far the square moved between two measurements, in mm.
fn shift(from: &Measurement, to: &Measurement) -> f64 {
    hypot((to.dx_mm - from.dx_mm, to.dy_mm - from.dy_mm))
}
