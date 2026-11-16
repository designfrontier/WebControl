import type { OpticalSettings } from "../../protocol";

/** The "Optical Calibration Settings" defaults from defaultwebcontrol.json. */
export const opticalSettings = (): OpticalSettings => ({
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
  feedRate: 500,
  settleTime: 1,
});
