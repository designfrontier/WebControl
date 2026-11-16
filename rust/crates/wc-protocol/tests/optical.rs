//! The optical wire types against their closed schemas, and the requests the UI sends.

use serde_json::{Value, json};
use wc_protocol::calibration::HoleyParameters;
use wc_protocol::optical::{
    CameraStatus, CaptureRequest, Cell, CenterFound, GeometryFit, Measurement, OpticalSettings, OpticalState,
    Orientation, Paper, PixelPoint, ScanImage, ScanProgress, ScanRequest, ScanStarted, ScanState, Session,
    SettingsSaved, SheetPose, TargetQuery, TestImage,
};
use wc_protocol::{Envelope, Message};
use wc_testkit::contract::{Contract, ResponseBody};

fn settings_json() -> Value {
    json!({"markerX": 0.6, "markerY": 0.6, "scaleX": 1.0, "scaleY": 1.0, "opticalCenterX": 0.0,
        "opticalCenterY": 0.0, "positionTolerance": 0.125, "tlX": -15, "tlY": 7, "brX": 15, "brY": -7,
        "calibrationExtents": "Full Area", "autoScanDirection": 0, "gaussianBlurValue": 5, "cannyLowValue": 50.0,
        "cannyHighValue": 100.0, "feedRate": 500.0, "settleTime": 1.0})
}

fn settings() -> OpticalSettings {
    serde_json::from_value(settings_json()).unwrap()
}

fn measurement() -> Measurement {
    Measurement {
        dx_mm: 0.4,
        dy_mm: -0.2,
        centre_px: PixelPoint { x: 321.5, y: 239.0 },
        angle_deg: 1.5,
        frames_used: 9,
    }
}

fn holey() -> HoleyParameters {
    HoleyParameters {
        motor_yoffset: 463.0,
        distance_between_motors: 2978.5,
        left_chain_tolerance: 0.0,
        right_chain_tolerance: 0.0,
    }
}

fn answers(method: &str, path: &str, value: &impl serde::Serialize) {
    let body = ResponseBody::Json(serde_json::to_value(value).unwrap());
    Contract::shared().validate_response(method, path, 200, &body).unwrap();
}

fn pushes<M: Message>(message: &M) -> Value {
    let envelope = serde_json::to_value(Envelope::json(message)).unwrap();
    Contract::shared().validate_envelope(&envelope).unwrap();
    serde_json::from_str(envelope["data"].as_str().unwrap()).unwrap()
}

#[test]
fn the_scan_broadcasts_fit_their_closed_schemas() {
    let cell = Cell { i: -15, j: 7 };
    let progress = ScanProgress {
        state: ScanState::Measuring,
        cell: Some(cell),
        attempt: 1,
        done: 3,
        total: 465,
        measurement: Some(measurement()),
        error: Some("No square found".into()),
    };
    assert_eq!(pushes(&progress)["measurement"]["centrePx"], json!({"x": 321.5, "y": 239.0}));
    let idle = ScanProgress { state: ScanState::Idle, cell: None, measurement: None, error: None, ..progress };
    assert_eq!(pushes(&idle), json!({"state": "idle", "cell": null, "attempt": 1, "done": 3, "total": 465}));
    let orientation = Orientation { rotation_deg: 0.5, mirrored: false, px_per_mm: 6.3 };
    assert_eq!(pushes(&orientation)["pxPerMm"], 6.3);
    assert_eq!(pushes(&ScanImage { cell, png: "iVBO".into() })["cell"], json!({"i": -15, "j": 7}));
}

#[test]
fn the_answers_fit_their_closed_schemas() {
    let row = |value| vec![value; 15];
    let mut error_x = vec![row(None); 31];
    error_x[0][0] = Some(0.25);
    let session = Session {
        state: ScanState::Stopped,
        settings: settings(),
        error_x: error_x.clone(),
        error_y: error_x,
        orientation: None,
    };
    let state = OpticalState {
        settings: settings(),
        session: Some(session),
        camera: CameraStatus { source: "auto".into(), available: false },
    };
    answers("GET", "/api/calibration/optical", &state);
    answers("GET", "/api/calibration/optical", &OpticalState { session: None, ..state });
    answers("PUT", "/api/calibration/optical/settings", &SettingsSaved { settings: settings() });
    answers("POST", "/api/calibration/optical/scan", &ScanStarted { total: 465 });
    let image = TestImage { png: "iVBO".into(), measurement: Some(measurement()) };
    answers("POST", "/api/calibration/optical/test-image", &image);
    answers("POST", "/api/calibration/optical/test-image", &TestImage { measurement: None, ..image });
    let centre = CenterFound { optical_center_x: 320.0, optical_center_y: 240.0 };
    answers("POST", "/api/calibration/optical/find-center", &centre);
    let fit = GeometryFit {
        current: holey(),
        fitted: holey(),
        sheet: SheetPose { x_mm: 1.0, y_mm: -2.0, rotation_deg: 0.1 },
        rms_mm: 0.3,
        cells: 465,
    };
    answers("POST", "/api/calibration/optical/fit", &fit);
    answers("POST", "/api/calibration/optical/accept", &holey());
}

#[test]
fn the_requests_the_ui_sends_decode() {
    let contract = Contract::shared();
    let scan = json!({"settings": settings_json(), "cell": {"i": 2, "j": -3}});
    contract.validate_request("POST", "/api/calibration/optical/scan", &scan).unwrap();
    let request: ScanRequest = serde_json::from_value(scan).unwrap();
    assert_eq!((request.cell, request.settings.tl_x), (Some(Cell { i: 2, j: -3 }), -15));
    let whole: ScanRequest = serde_json::from_value(json!({"settings": settings_json()})).unwrap();
    assert_eq!(whole.cell, None);
    let capture = json!({"settings": settings_json()});
    contract.validate_request("POST", "/api/calibration/optical/test-image", &capture).unwrap();
    let capture: CaptureRequest = serde_json::from_value(capture).unwrap();
    assert_eq!(capture.settings.feed_rate, 500.0);
    contract.validate_request("PUT", "/api/calibration/optical/settings", &settings_json()).unwrap();
    assert_eq!(serde_json::to_value(settings()).unwrap(), settings_json());
}

#[test]
fn the_target_paper_defaults_to_letter() {
    let query = |value| serde_json::from_value::<TargetQuery>(value);
    assert_eq!(query(json!({})).unwrap(), TargetQuery { paper: Paper::Letter });
    assert_eq!(query(json!({"paper": "a4"})).unwrap(), TargetQuery { paper: Paper::A4 });
    assert!(query(json!({"paper": "legal"})).is_err());
}
