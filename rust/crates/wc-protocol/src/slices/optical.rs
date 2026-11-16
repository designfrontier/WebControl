//! The optical part of the `calibration` slice: experimental, `spec/optical-calibration.md`.

use serde::{Deserialize, Serialize};

use crate::Message;
use crate::calibration::HoleyParameters;

/// A target square: column `i` in -15..=15 (+X right), row `j` in -7..=7 (+Y up), 76.2 mm apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Cell {
    pub i: i32,
    pub j: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PixelPoint {
    pub x: f64,
    pub y: f64,
}

/// One square's offset from the optical centre in machine mm (+Y up), averaged over `frames_used` frames.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Measurement {
    pub dx_mm: f64,
    pub dy_mm: f64,
    pub centre_px: PixelPoint,
    pub angle_deg: f64,
    pub frames_used: u32,
}

/// The camera's mounting as the pre-check measured it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Orientation {
    pub rotation_deg: f64,
    pub mirrored: bool,
    pub px_per_mm: f64,
}

impl Message for Orientation {
    const COMMAND: &'static str = "opticalOrientation";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScanState {
    Idle,
    Orienting,
    Moving,
    Settling,
    Measuring,
    Failed,
    Done,
    Stopped,
}

/// `opticalScanProgress`, broadcast on every state change of a scan.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScanProgress {
    pub state: ScanState,
    pub cell: Option<Cell>,
    pub attempt: u32,
    pub done: u32,
    pub total: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub measurement: Option<Measurement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Message for ScanProgress {
    const COMMAND: &'static str = "opticalScanProgress";
}

/// `opticalImage`: the annotated frame of the square just measured, base64 PNG.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScanImage {
    pub cell: Cell,
    pub png: String,
}

impl Message for ScanImage {
    const COMMAND: &'static str = "opticalImage";
}

/// The "Optical Calibration Settings" section; marker sizes in inches, tolerance in mm, feed in mm/min.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpticalSettings {
    pub marker_x: f64,
    pub marker_y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub optical_center_x: f64,
    pub optical_center_y: f64,
    pub position_tolerance: f64,
    pub tl_x: i32,
    pub tl_y: i32,
    pub br_x: i32,
    pub br_y: i32,
    pub calibration_extents: String,
    pub auto_scan_direction: i32,
    pub gaussian_blur_value: i32,
    pub canny_low_value: f64,
    pub canny_high_value: f64,
    pub feed_rate: f64,
    pub settle_time: f64,
}

/// A scan's result so far; the grids are 31 x 15, indexed `[i + 15][7 - j]`, `None` where not measured.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub state: ScanState,
    pub settings: OpticalSettings,
    pub error_x: Vec<Vec<Option<f64>>>,
    pub error_y: Vec<Vec<Option<f64>>>,
    pub orientation: Option<Orientation>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CameraStatus {
    pub source: String,
    pub available: bool,
}

/// `GET /api/calibration/optical`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OpticalState {
    pub settings: OpticalSettings,
    pub session: Option<Session>,
    pub camera: CameraStatus,
}

/// `PUT /api/calibration/optical/settings`'s answer.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SettingsSaved {
    pub settings: OpticalSettings,
}

/// `POST /api/calibration/optical/scan`; with `cell`, only that square is measured.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ScanRequest {
    pub settings: OpticalSettings,
    #[serde(default)]
    pub cell: Option<Cell>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ScanStarted {
    pub total: u32,
}

/// `POST /api/calibration/optical/test-image` and `/find-center`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CaptureRequest {
    pub settings: OpticalSettings,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TestImage {
    pub png: String,
    pub measurement: Option<Measurement>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CenterFound {
    pub optical_center_x: f64,
    pub optical_center_y: f64,
}

/// Where the printed target lies on the machine, as the geometry fit found it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetPose {
    pub x_mm: f64,
    pub y_mm: f64,
    pub rotation_deg: f64,
}

/// `POST /api/calibration/optical/fit`: the holey geometry that best explains the grid, unsaved.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeometryFit {
    pub current: HoleyParameters,
    pub fitted: HoleyParameters,
    pub sheet: SheetPose,
    pub rms_mm: f64,
    pub cells: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Paper {
    #[default]
    Letter,
    A4,
}

/// `GET /api/calibration/optical/target.pdf`'s query.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct TargetQuery {
    #[serde(default)]
    pub paper: Paper,
}
