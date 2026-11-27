import { z } from "zod";
import { actionArg, numberLike } from "./core";
import type { ActionCommand } from "./index";

/**
 * When processAction runs a command: "always"; "idleOrPaused" refuses while gcode streams;
 * "idleOrControllerPaused" also while Pause holds the job (Play, which resumes a controller pause); "idle" also while
 * the controller paused the job; "noJob" also while a tool change waits. A refusal is an Alert.
 */
export type AllowedDuring = "always" | "idleOrPaused" | "idleOrControllerPaused" | "idle" | "noJob";

/** How the UI reaches processAction for a command; most go through the `action` socket event. */
export type ActionRoute = "action" | "POST /api/system/pid-test" | "none";

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
const noJob = (summary: string, args?: Parameters<typeof spec>[2]) => spec("noJob", summary, args);

const direction = z.enum(["upLeft", "up", "upRight", "left", "right", "downLeft", "down", "downRight"]);

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
  zToZero: notRunning("Move the bit to Z0, once Z zero is defined (Rust only)"),
  zToSafeHeight: notRunning("Raise the bit to the safe height (Rust only)"),
  moveToDefault: notRunning("Move to the default chain lengths"),
  returnToCenter: notRunning("Move the sled to the bed centre"),
  moveGcodeZ: notRunning("Step the gcode index to the next or previous Z move", {
    arg: integer.describe("Number of Z moves to step, signed"),
  }),
  moveGcodeGoto: notRunning("Set the gcode index", { arg: integer.describe("Zero-based line index") }),
  moveGcodeIndex: notRunning("Step the gcode index", { arg: integer.describe("Lines to step, signed") }),
  macro1: notRunning("Run macro 1"),
  macro2: notRunning("Run macro 2"),
  macro3: notRunning("Run macro 3 (Rust only)"),
  macro4: notRunning("Run macro 4 (Rust only)"),
  clearLogs: notRunning("Delete old log files"),
  clearGCode: noJob("Unload the gcode"),
  startRun: spec(
    "idleOrControllerPaused",
    "Start sending the loaded gcode, or resume a job the controller paused or a tool change",
  ),
  update: idle("Install a WebControl release and restart", { arg: z.string().describe("Release tag") }),
  cutTriangularCalibrationPattern: idle("Cut the triangular calibration pattern"),
  acceptTriangularCalibrationResults: idle("Store the last triangular calibration result"),
  cutHoleyCalibrationPattern: idle("Cut the holey calibration pattern"),
  acceptHoleyCalibrationResults: idle("Store the last holey calibration result"),
  opticalRetryCell: idle("Measure the optical square that failed again"),
  opticalSkipCell: idle("Leave the optical square that failed unmeasured and go on"),
  opticalClearSession: idle("Forget the optical scan's measurements"),
  resetChainLengths: idle("Tell the controller the chains are at their default lengths"),
  defineHome: noJob("Define home at a point, or at the sled when both args are empty", {
    arg: z.union([z.number(), z.literal("")]).describe('X in inches, or "" for the sled position'),
    arg1: z.union([z.number(), z.literal("")]).describe('Y in inches, or "" for the sled position'),
  }),
  testMotors: idle("Run each motor in both directions"),
  wipeEEPROM: idle("Erase controller EEPROM", { arg: z.enum(["All", "Settings", "Maslow"]) }),
  setSprockets: idle("Turn a sprocket by an angle", { arg: motor, arg1: z.number().describe("Degrees, signed") }),
  rotateSprocket: idle("Run a motor at full speed for a time", {
    arg: motor,
    arg1: z.number().describe("Seconds; the sign sets the direction"),
  }),
  setSprocketAutomatic: idle("Turn the sprockets to vertical automatically"),
  setSprocketsZero: idle("Define the current sprocket angles as zero"),
  setSprocketsDefault: idle("Mark the chains as at their default lengths"),
  updatePorts: idle("Rescan serial ports; answered by an updatePorts message"),
  upgradeCustomFirmware: idle("Flash the bundled custom firmware"),
  upgradeStockFirmware: idle("Flash the bundled stock firmware"),
  upgradeHoleyFirmware: idle("Flash the bundled holey firmware", {
    arg: actionArg.describe(
      'Python flashes the bundled hex and ignores it (the UI sends ""). webcontrol-rs runs the guided upgrade one ' +
        'step per action: "check" (for 51.292) or "checkPlanner" (for the planner 51.30), then "upgrade", ' +
        '"selfTest", "rollback" or "reset"; "" only alerts.',
    ),
  }),
  adjustChain: idle("Extend one chain after a 5 s countdown", { arg: z.enum(["left", "right"]) }),
  executeVelocityPIDTest: noJob("Start a velocity PID test", {
    arg: pidArgs("v", ["KpV", "KiV", "KdV"], false),
    via: "POST /api/system/pid-test",
  }),
  executePositionPIDTest: noJob("Start a position PID test", {
    arg: pidArgs("p", ["KpP", "KiP", "KdP"], true),
    via: "POST /api/system/pid-test",
  }),
  boardProcessGCode: idle("Mark the loaded gcode's area as cut on the current board"),
  boardClearBoard: idle("Clear the current board's cut area"),
  updatePyInstaller: idle("Install the newer PyInstaller release and restart"),
  setFakeServo: idle("Turn the controller's fake servo on or off", {
    arg: z.union([z.literal(0), z.literal(1)]),
  }),
  resetHomeToCenter: noJob("Move home back to the bed centre"),
  park: idle("Raise Z to the safe height and rapid to the park position (Rust only)"),
  definePark: noJob("Make the sled's current position the park position (Rust only)"),
} satisfies Record<ActionCommand, ActionSpec>;
