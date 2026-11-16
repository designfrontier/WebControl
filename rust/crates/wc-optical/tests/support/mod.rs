//! Shared by the optical tests: settings with a non-square marker and printer scale.

#![allow(dead_code)]

use wc_optical::OpticalSettings;

pub fn settings() -> OpticalSettings {
    serde_json::from_value(serde_json::json!({"markerX": 0.6, "markerY": 0.5, "scaleX": 1.0, "scaleY": 0.99,
        "opticalCenterX": 0.0, "opticalCenterY": 0.0, "positionTolerance": 0.125, "tlX": -15, "tlY": 7, "brX": 15,
        "brY": -7, "calibrationExtents": "Full Area", "autoScanDirection": 0, "gaussianBlurValue": 5,
        "cannyLowValue": 50.0, "cannyHighValue": 100.0, "feedRate": 500.0, "settleTime": 1.0}))
    .unwrap()
}
