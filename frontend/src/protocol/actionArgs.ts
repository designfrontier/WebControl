import { z } from "zod";
import { opticalSettingsSchema } from "./calibration";
import { actionArg, numberLike } from "./core";
import type { ActionCommand } from "./index";

/**
 * When processAction runs a command: "always"; "idleOrPaused" refuses while gcode streams (uploadFlag 1);
 * "idle" also refuses while a job is paused. A refusal is an Alert, never an error reply.
 */
export type AllowedDuring = "always" | "idleOrPaused" | "idle";

/** How the UI reaches processAction for a command; most go through the `action` socket event. */
export type ActionRoute = "action" | "opticalCalibration" | "POST /api/system/pid-test" | "none";

export interface ActionSpec {
  allowedDuring: AllowedDuring;
  arg: z.ZodType;
  arg1: z.ZodType;
  via: ActionRoute;
  summary: string;
}

const ignored = actionArg.describe('Ignored by the backend; the UI sends "".');
const integer = z.union([z.number().int(), z.string().regex(/^\s*[-+]?\d+\s*$/)]);
const motor = z.string().describe('Motor letter: "L", "R", or on four-motor firmware "M", "S"/"N".');
const pidArgs = (prefix: "v" | "p", gains: [string, string, string], time: boolean) =>
  z.object({
    [`${prefix}Motor`]: z.string(),
    [`${prefix}Start`]: z.string(),
    [`${prefix}Stop`]: z.string(),
    [`${prefix}Steps`]: z.string(),
    ...(time ? { [`${prefix}Time`]: z.string() } : {}),
    [`${prefix}Version`]: z.string(),
    ...Object.fromEntries(gains.map((gain) => [gain, z.string()])),
  });

const spec = (
  allowedDuring: AllowedDuring,
  summary: string,
  { arg = ignored, arg1 = ignored, via = "action" }: { arg?: z.ZodType; arg1?: z.ZodType; via?: ActionRoute } = {},
): ActionSpec => ({ allowedDuring, summary, arg, arg1, via });

const always = (summary: string, args?: Parameters<typeof spec>[2]) => spec("always", summary, args);
const notRunning = (summary: string, args?: Parameters<typeof spec>[2]) => spec("idleOrPaused", summary, args);
const idle = (summary: string, args?: Parameters<typeof spec>[2]) => spec("idle", summary, args);

const direction = z.enum(["upLeft", "up", "upRight", "left", "right", "downLeft", "down", "downRight"]);
const opticalCamera = { arg: opticalSettingsSchema, via: "opticalCalibration" as const };

/** processAction's `arg`/`arg1` for each command, in the order Actions.processAction tests them. */
export const actionSpecs = {
  createDirectory: always("Create a gcode library directory", { arg: z.string().describe("Directory name") }),
  stopZ: always("Stop a Z-axis move"),
  stopRun: always("Stop the job and clear the controller's buffer"),
  pauseRun: always("Pause the job"),
  resumeRun: always("Resume a paused job or tool change"),
  toggleCamera: always("Turn the webcam stream on or off"),
  statusRequest: always("Ask for a status push", {
    arg: z.string().describe('Only "cameraStatus" does anything: it answers with a cameraMessage updateCamera.'),
  }),
  queryCamera: always("Ask whether the camera is streaming"),
  shutdown: always("Stop WebControl"),
  TurnOffRPI: always("Power off the Raspberry Pi"),
  move: notRunning("Jog the sled", { arg: direction, arg1: numberLike.describe("Distance in the front-page units") }),
  moveTo: notRunning("Move the sled to a point picked on the viewer", {
    arg: z.number().describe("X in inches from the bed centre"),
    arg1: numberLike.describe("Y in inches from the bed centre"),
  }),
  moveZ: notRunning("Raise or lower the bit", {
    arg: z.enum(["raise", "lower"]),
    arg1: numberLike.describe("Distance in the Z units"),
  }),
  setMaxZ: notRunning("Store the current Z as the upper limit"),
  setMinZ: notRunning("Store the current Z as the lower limit"),
  clearZ: notRunning("Clear the Z limits"),
  getZlimits: notRunning("Ask the controller to report its Z limits"),
  reportSettings: notRunning("Ask the controller for its settings ($$)"),
  home: notRunning("Move the sled to home"),
  defineZ0: notRunning("Define the current Z as zero"),
  touchZ: notRunning("Probe Z with the touch plate"),
  moveToDefault: notRunning("Move to the default chain lengths"),
  returnToCenter: notRunning("Move the sled to the bed centre"),
  moveGcodeZ: notRunning("Step the gcode index to the next or previous Z move", {
    arg: integer.describe("Number of Z moves to step, signed"),
  }),
  moveGcodeGoto: notRunning("Set the gcode index", { arg: integer.describe("Zero-based line index") }),
  moveGcodeIndex: notRunning("Step the gcode index", { arg: integer.describe("Lines to step, signed") }),
  macro1: notRunning("Run macro 1"),
  macro2: notRunning("Run macro 2"),
  clearLogs: notRunning("Delete old log files"),
  startRun: idle("Start sending the loaded gcode"),
  update: idle("Install a WebControl release and restart", { arg: z.string().describe("Release tag") }),
  cutTriangularCalibrationPattern: idle("Cut the triangular calibration pattern"),
  acceptTriangularCalibrationResults: idle("Store the last triangular calibration result"),
  cutHoleyCalibrationPattern: idle("Cut the holey calibration pattern"),
  acceptHoleyCalibrationResults: idle("Store the last holey calibration result"),
  resetChainLengths: idle("Tell the controller the chains are at their default lengths"),
  defineHome: idle("Define home at a point, or at the sled when both args are empty", {
    arg: z.union([z.number(), z.literal("")]).describe('X in inches, or "" for the sled position'),
    arg1: z.union([z.number(), z.literal("")]).describe('Y in inches, or "" for the sled position'),
  }),
  testMotors: idle("Run each motor in both directions"),
  wipeEEPROM: idle("Erase controller EEPROM", { arg: z.enum(["All", "Settings", "Maslow"]) }),
  clearGCode: idle("Unload the gcode"),
  setSprockets: idle("Turn a sprocket by an angle", { arg: motor, arg1: z.number().describe("Degrees, signed") }),
  rotateSprocket: idle("Run a motor at full speed for a time", {
    arg: motor,
    arg1: z.number().describe("Seconds; the sign sets the direction"),
  }),
  setSprocketAutomatic: idle("Turn the sprockets to vertical automatically"),
  setSprocketsZero: idle("Define the current sprocket angles as zero"),
  setSprocketsDefault: idle("Mark the chains as at their default lengths"),
  updatePorts: idle("Rescan serial ports; answered by an updatePorts message"),
  optical_onStart: idle("Start the optical calibration camera"),
  optical_Calibrate: idle("Run optical calibration", opticalCamera),
  saveOpticalCalibrationConfiguration: idle("Save optical settings", {
    arg: opticalSettingsSchema,
    via: "none",
  }),
  stopOpticalCalibration: idle("Stop optical calibration"),
  testOpticalCalibration: idle("Grab a test image", opticalCamera),
  findCenterOpticalCalibration: idle("Find the optical centre", opticalCamera),
  saveAndSendOpticalCalibration: idle("Store the calibration and send it to the controller"),
  reloadCalibration: idle("Reload the stored calibration; answered by updateOpticalCalibrationError"),
  saveCalibrationToCSV: idle("Write the calibration to CSV"),
  clearCalibration: idle("Clear the calibration"),
  curveFitOpticalCalibration: idle("Fit curves; answered by updateOpticalCalibrationCurve"),
  adjustCenter: idle("Move the optical centre up or down and return to it", {
    arg: z.number().describe("mm, signed"),
  }),
  upgradeCustomFirmware: idle("Flash the bundled custom firmware"),
  upgradeStockFirmware: idle("Flash the bundled stock firmware"),
  upgradeHoleyFirmware: idle("Flash the bundled holey firmware"),
  adjustChain: idle("Extend one chain after a 5 s countdown", { arg: z.enum(["left", "right"]) }),
  executeVelocityPIDTest: idle("Start a velocity PID test", {
    arg: pidArgs("v", ["KpV", "KiV", "KdV"], false),
    via: "POST /api/system/pid-test",
  }),
  executePositionPIDTest: idle("Start a position PID test", {
    arg: pidArgs("p", ["KpP", "KiP", "KdP"], true),
    via: "POST /api/system/pid-test",
  }),
  boardProcessGCode: idle("Mark the loaded gcode's area as cut on the current board"),
  boardClearBoard: idle("Clear the current board's cut area"),
  updatePyInstaller: idle("Install the newer PyInstaller release and restart"),
  setFakeServo: idle("Turn the controller's fake servo on or off", {
    arg: z.union([z.literal(0), z.literal(1)]),
  }),
  resetHomeToCenter: idle("Move home back to the bed centre"),
} satisfies Record<ActionCommand, ActionSpec>;
