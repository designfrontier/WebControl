import { z } from "zod";
import type { EventSchemas, MessageSchemas } from "./core";

const grid = z.array(z.array(z.number()));

/** Server -> client commands for triangular, holey and optical calibration. */
export const calibrationMessages = {
  updateOpticalCalibrationCurve: z.object({ curveX: z.array(z.number()), curveY: z.array(z.number()) }),
  updateOpticalCalibrationError: z.object({ errorX: grid, errorY: grid }),
  updateOpticalCalibrationFindCenter: z.looseObject({}),
  updateCalibrationImage: z.object({ command: z.string(), data: z.unknown() }),
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

export const calibrationEvents = {} satisfies EventSchemas;
