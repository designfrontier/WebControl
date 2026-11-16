import { z } from "zod";
import { numberLike } from "./core";
import type { EventSchemas, MessageSchemas } from "./core";

export const opticalCellSchema = z
  .object({ i: z.number().int().min(-15).max(15), j: z.number().int().min(-7).max(7) })
  .describe("A target square: column i (+X right), row j (+Y up), 3 inches apart");
export type OpticalCell = z.infer<typeof opticalCellSchema>;

/** One square's offset from the optical centre, averaged over the frames that found it. */
export const opticalMeasurementSchema = z.object({
  dxMm: z.number(),
  dyMm: z.number(),
  centrePx: z.object({ x: z.number(), y: z.number() }),
  angleDeg: z.number(),
  framesUsed: z.number().int(),
});
export type OpticalMeasurement = z.infer<typeof opticalMeasurementSchema>;

/** What the orientation pre-check measured from a +5 mm X move and a +5 mm Y move. */
export const opticalOrientationSchema = z.object({
  rotationDeg: z.number(),
  mirrored: z.boolean(),
  pxPerMm: z.number(),
});
export type OpticalOrientation = z.infer<typeof opticalOrientationSchema>;

export const opticalScanStates = [
  "idle",
  "orienting",
  "moving",
  "settling",
  "measuring",
  "failed",
  "done",
  "stopped",
] as const;
const scanState = z.enum(opticalScanStates);

/** Server -> client broadcasts of an optical scan; triangular and holey calibration answer over HTTP. */
export const calibrationMessages = {
  opticalScanProgress: z.object({
    state: scanState,
    cell: opticalCellSchema.nullable(),
    attempt: z.number().int(),
    done: z.number().int(),
    total: z.number().int(),
    measurement: opticalMeasurementSchema.optional(),
    error: z.string().optional().describe("Why the cell failed; the scan waits for Retry, Skip or Stop"),
  }),
  opticalOrientation: opticalOrientationSchema,
  opticalImage: z.object({ cell: opticalCellSchema, png: z.string().describe("Base64 PNG of the annotated frame") }),
} satisfies MessageSchemas;

export const calibrationActions = [
  "cutTriangularCalibrationPattern",
  "acceptTriangularCalibrationResults",
  "cutHoleyCalibrationPattern",
  "acceptHoleyCalibrationResults",
  "opticalRetryCell",
  "opticalSkipCell",
  "opticalClearSession",
] as const;

export const calibrationEvents = {} satisfies EventSchemas;

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

/** The "Optical Calibration Settings" section; a scan uses the copy in its request, not the stored one. */
export const opticalSettingsSchema = z.object({
  markerX: z.number().describe("Printed square size, inches"),
  markerY: z.number(),
  scaleX: z.number().describe("Printer scale error of the target"),
  scaleY: z.number(),
  opticalCenterX: z.number().describe("Pixel of the sled reference; 0 means the image centre"),
  opticalCenterY: z.number(),
  positionTolerance: z.number().describe("mm"),
  tlX: z.number().int(),
  tlY: z.number().int(),
  brX: z.number().int(),
  brY: z.number().int(),
  calibrationExtents: z.string(),
  autoScanDirection: z.number().int().describe("0 horizontal serpentine, 1 vertical"),
  gaussianBlurValue: z.number().int(),
  cannyLowValue: z.number(),
  cannyHighValue: z.number(),
  feedRate: z.number().describe("mm/min of the G1 moves between squares"),
  settleTime: z.number().describe("Seconds to wait after a move before capturing"),
});
export type OpticalSettings = z.infer<typeof opticalSettingsSchema>;

const errorGrid = z
  .array(z.array(z.number().nullable()))
  .describe("31 columns x 15 rows of mm, indexed [i+15][7-j]; null where not measured");

export const opticalSessionSchema = z.object({
  state: scanState,
  settings: opticalSettingsSchema,
  errorX: errorGrid,
  errorY: errorGrid,
  orientation: opticalOrientationSchema.nullable(),
});
export type OpticalSession = z.infer<typeof opticalSessionSchema>;

/** GET /api/calibration/optical. */
export const opticalStateSchema = z.object({
  settings: opticalSettingsSchema,
  session: opticalSessionSchema.nullable(),
  camera: z.object({ source: z.string(), available: z.boolean() }),
});
export type OpticalState = z.infer<typeof opticalStateSchema>;

export const opticalSettingsSavedSchema = z.object({ settings: opticalSettingsSchema });
export const opticalScanRequestSchema = z.object({
  settings: opticalSettingsSchema,
  cell: opticalCellSchema.optional().describe("Measure only this square"),
});
export const opticalScanStartedSchema = z.object({ total: z.number().int() });
export const opticalCaptureRequestSchema = z.object({ settings: opticalSettingsSchema });
export const opticalTestImageSchema = z.object({
  png: z.string().describe("Base64 PNG"),
  measurement: opticalMeasurementSchema.nullable(),
});
export const opticalCenterSchema = z.object({ opticalCenterX: z.number(), opticalCenterY: z.number() });

/** POST /api/calibration/optical/fit: the holey geometry that best explains the grid, unsaved. */
export const opticalFitSchema = z.object({
  current: holeyParametersSchema,
  fitted: holeyParametersSchema,
  sheet: z.object({ xMm: z.number(), yMm: z.number(), rotationDeg: z.number() }),
  rmsMm: z.number(),
  cells: z.number().int(),
});
export type OpticalFit = z.infer<typeof opticalFitSchema>;
export const opticalPaperQuerySchema = z.object({ paper: z.enum(["letter", "a4"]).optional() });

/** The body of a 4xx/5xx from /api/calibration. */
export const calibrationErrorSchema = z.object({ error: z.string() });

/** POST /api/calibration/triangular body, in mm. */
export const triangularMeasurementsSchema = z.object({
  cut12: numberLike,
  cut34: numberLike,
  cut5: numberLike,
  bitDiameter: numberLike,
});

/** POST /api/calibration/holey body: M1 to M12 in mm. */
export const holeyMeasurementsSchema = z.object({ measurements: z.array(numberLike).length(12) });
