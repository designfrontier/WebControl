import type { OpticalState, OpticalSettings } from "../../protocol";

/** Test data for the calibration specs. */
export const opticalSettings = (over: Partial<OpticalSettings> = {}): OpticalSettings => ({
  markerX: 0.6,
  markerY: 0.6,
  scaleX: 1,
  scaleY: 1,
  opticalCenterX: 0,
  opticalCenterY: 0,
  positionTolerance: 0.125,
  tlX: -15,
  tlY: 7,
  brX: 15,
  brY: -7,
  calibrationExtents: "Full Area",
  autoScanDirection: 0,
  gaussianBlurValue: 5,
  cannyLowValue: 50,
  cannyHighValue: 100,
  ...over,
});

export const opticalState = (over: Partial<OpticalState> = {}): OpticalState => ({
  settings: opticalSettings(),
  curve: { curveX: [0, 0, 0, 0, 0, 0], curveY: [0, 0, 0, 0, 0, 0] },
  error: { errorX: [], errorY: [] },
  ...over,
});
