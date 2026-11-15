import { z } from "zod";
import { numberLike } from "./core";
import type { EventSchemas, MessageSchemas } from "./core";

/** Triangular and holey calibration answer over HTTP, so no server -> client commands. */
export const calibrationMessages = {} satisfies MessageSchemas;

export const calibrationActions = [
  "cutTriangularCalibrationPattern",
  "acceptTriangularCalibrationResults",
  "cutHoleyCalibrationPattern",
  "acceptHoleyCalibrationResults",
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
