import { z } from "zod";
import type { EventSchemas, MessageSchemas } from "./core";

const grid = z.array(z.array(z.number()));
const curve = z.array(z.number());

/** Server -> client commands for triangular, holey and optical calibration. */
export const calibrationMessages = {
  updateOpticalCalibrationCurve: z.object({ curveX: curve, curveY: curve }),
  updateOpticalCalibrationError: z.object({ errorX: grid, errorY: grid }),
  updateOpticalCalibrationFindCenter: z.looseObject({ opticalCenterX: z.number(), opticalCenterY: z.number() }),
  updateCalibrationImage: z.object({
    command: z.enum(["OpticalCalibrationImageUpdated", "OpticalCalibrationTestImageUpdated"]),
    data: z.string(),
  }),
} satisfies MessageSchemas;

export const calibrationActions = [
  "cutTriangularCalibrationPattern",
  "acceptTriangularCalibrationResults",
  "cutHoleyCalibrationPattern",
  "acceptHoleyCalibrationResults",
  "optical_onStart",
  "optical_Calibrate",
  "saveOpticalCalibrationConfiguration",
  "stopOpticalCalibration",
  "testOpticalCalibration",
  "findCenterOpticalCalibration",
  "saveAndSendOpticalCalibration",
  "reloadCalibration",
  "saveCalibrationToCSV",
  "clearCalibration",
  "curveFitOpticalCalibration",
  "adjustCenter",
] as const;

/** What GET /api/calibration/optical returns and the optical camera actions take as their argument. */
export const opticalSettingsSchema = z.object({
  markerX: z.number(),
  markerY: z.number(),
  scaleX: z.number(),
  scaleY: z.number(),
  opticalCenterX: z.number(),
  opticalCenterY: z.number(),
  positionTolerance: z.number(),
  tlX: z.number().int(),
  tlY: z.number().int(),
  brX: z.number().int(),
  brY: z.number().int(),
  calibrationExtents: z.string(),
  autoScanDirection: z.number().int(),
  gaussianBlurValue: z.number().int(),
  cannyLowValue: z.number(),
  cannyHighValue: z.number(),
});
export type OpticalSettings = z.infer<typeof opticalSettingsSchema>;

export const opticalCameraCommands = [
  "optical_Calibrate",
  "testOpticalCalibration",
  "findCenterOpticalCalibration",
] as const;
export type OpticalCameraCommand = (typeof opticalCameraCommands)[number];

export const calibrationEvents = {
  /** processAction for the camera actions, whose argument is the settings object rather than a scalar. */
  opticalCalibration: z.object({
    data: z.object({ command: z.enum(opticalCameraCommands), arg: opticalSettingsSchema }),
  }),
} satisfies EventSchemas;

export const triangularParametersSchema = z.object({
  motorYoffset: z.number(),
  rotationRadius: z.number(),
  chainSagCorrection: z.number(),
});
export const triangularResultSchema = triangularParametersSchema.extend({ calibrationError: z.number() });
export type TriangularParameters = z.infer<typeof triangularParametersSchema>;
export type TriangularResult = z.infer<typeof triangularResultSchema>;

export const holeyParametersSchema = z.object({
  motorYoffset: z.number(),
  distanceBetweenMotors: z.number(),
  leftChainTolerance: z.number(),
  rightChainTolerance: z.number(),
});
export const holeyResultSchema = holeyParametersSchema.extend({ calibrationError: z.number() });
export type HoleyParameters = z.infer<typeof holeyParametersSchema>;
export type HoleyResult = z.infer<typeof holeyResultSchema>;

export const opticalCurveSchema = calibrationMessages.updateOpticalCalibrationCurve;
export const opticalErrorSchema = calibrationMessages.updateOpticalCalibrationError;
export type OpticalCurve = z.infer<typeof opticalCurveSchema>;
export type OpticalError = z.infer<typeof opticalErrorSchema>;

export const opticalStateSchema = z.object({
  settings: opticalSettingsSchema,
  curve: opticalCurveSchema,
  error: opticalErrorSchema,
});
export type OpticalState = z.infer<typeof opticalStateSchema>;

/** PUT /api/calibration/optical/settings answers with what it stored. */
export const opticalSettingsSavedSchema = z.object({ settings: opticalSettingsSchema });

/** The body of a 4xx/5xx from /api/calibration. */
export const calibrationErrorSchema = z.object({ error: z.string() });
