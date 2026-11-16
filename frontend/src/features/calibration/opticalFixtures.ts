import type { OpticalFit, OpticalSession, OpticalSettings, OpticalState } from "../../protocol";

/** The "Optical Calibration Settings" defaults from defaultwebcontrol.json. */
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
  feedRate: 500,
  settleTime: 1,
  ...over,
});

/** A 31 x 15 grid indexed [i+15][7-j] with `value(i, j)` in each square. */
export const errorGrid = (value: (i: number, j: number) => number | null = () => null) =>
  Array.from({ length: 31 }, (_, column) => Array.from({ length: 15 }, (_, row) => value(column - 15, 7 - row)));

export const opticalSession = (over: Partial<OpticalSession> = {}): OpticalSession => ({
  state: "idle",
  settings: opticalSettings(),
  errorX: errorGrid(),
  errorY: errorGrid(),
  orientation: null,
  ...over,
});

export const opticalState = (over: Partial<OpticalState> = {}): OpticalState => ({
  settings: opticalSettings(),
  session: null,
  camera: { source: "synthetic", available: true },
  ...over,
});

export const opticalFit = (over: Partial<OpticalFit> = {}): OpticalFit => ({
  current: { motorYoffset: 463, distanceBetweenMotors: 2978.4, leftChainTolerance: 0, rightChainTolerance: 0 },
  fitted: { motorYoffset: 458.1, distanceBetweenMotors: 2981.35, leftChainTolerance: 0.12, rightChainTolerance: -0.08 },
  sheet: { xMm: 1.2, yMm: -0.4, rotationDeg: 0.05 },
  rmsMm: 0.31,
  cells: 465,
  ...over,
});
