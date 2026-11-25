import { z } from "zod";
import { actionEventSchema, numeric } from "./core";
import type { EventSchemas, MessageSchemas } from "./core";
import { themeSchema } from "./settings";

const unitsSchema = z.enum(["MM", "INCHES"]);

const modalSchema = z.looseObject({
  title: z.string(),
  message: z.string(),
  modalType: z.string().optional(),
  modalSize: z.string().optional(),
  resume: z.union([z.string(), z.boolean()]).optional(),
  progress: z.union([z.string(), z.boolean()]).optional(),
  isStatic: z.boolean().optional(),
  client: z.string().optional(),
  /** The tool a tool-change notification asks for. Rust only. */
  tool: z.number().int().optional(),
});

export const PACING_MODES = ["v0.94", "minimum gap"] as const;
export const PACING_GAP_MAX_MS = 500;
export const pacingModeSchema = z.enum(PACING_MODES);
export type PacingMode = z.infer<typeof pacingModeSchema>;

/** The `[PE:]` reports taken while the sled moved: the larger of the two chain errors, in mm. */
export const positionErrorStatsSchema = z.object({
  samples: z.number(),
  meanMm: z.number(),
  maxMm: z.number(),
  over1Mm: z.number(),
});
export type PositionErrorStats = z.infer<typeof positionErrorStatsSchema>;

/** One stretch of a job streamed at one line pacing, or a whole job's totals. */
export const pacingSegmentSchema = z.object({
  /** For a job's totals, the pacing it started with. */
  mode: pacingModeSchema,
  gapMs: z.number(),
  /** Job lines written, not counting resends. */
  lines: z.number(),
  /** Lines the controller never received: each sent again or, lost twice, paused at. */
  lost: z.number(),
  lostTwice: z.number(),
  /** Wall-clock streaming time, recoveries included and pauses not. */
  elapsedS: z.number(),
  /** From writing each lost line to sending it again. */
  recoveryS: z.number(),
  linesPerMinute: z.number().nullable(),
  secondsPerLine: z.number().nullable(),
  lostPercent: z.number().nullable(),
  /** The median gap before a write whose line was acked within the pacing gap plus 100 ms. */
  medianShortGapMs: z.number().nullable(),
  /** Null while it runs. */
  endedBy: z.enum(["pacing change", "job end", "Stop"]).nullable(),
  /** Wall-clock time since it started, pauses included. */
  wallS: z.number().optional(),
  /** A running job's estimated time left, scaled by how its estimate compares with its time so far. */
  remainingS: z.number().nullable().optional(),
  /** Null until a report comes in while the sled moves. */
  positionError: positionErrorStatsSchema.nullable().optional(),
});
export type PacingSegment = z.infer<typeof pacingSegmentSchema>;

/** The line pacing and its statistics, pushed on every change and every 2 s during a job. Rust only. */
export const pacingStatsSchema = z.object({
  mode: pacingModeSchema,
  gapMs: z.number(),
  job: pacingSegmentSchema.nullable(),
  /** Null outside a job. */
  segment: pacingSegmentSchema.nullable(),
  /** The latest ended segments since the backend started, newest first. */
  history: z.array(pacingSegmentSchema),
  /** The loaded file's estimated run time from its moves and feeds; null with no file. */
  estimateS: z.number().nullable().optional(),
  /** The linePacingStats setting: whether the run panel shows the pacing. Absent reads as hidden. */
  showStats: z.boolean().optional(),
});
export type PacingStats = z.infer<typeof pacingStatsSchema>;

/** PUT /api/run/pacing body, any of its settings; allowed during a job, unlike the settings screens. */
export const pacingUpdateSchema = z.object({
  values: z.object({
    linePacingMode: pacingModeSchema.optional(),
    linePacingGapMs: z.number().int().min(0).max(PACING_GAP_MAX_MS).optional(),
    linePacingStats: z.boolean().optional(),
  }),
});
export type PacingUpdate = z.infer<typeof pacingUpdateSchema>;

/** The park position: mm from the bed's centre in machine coordinates, so Set Home never moves it. */
export const parkPositionSchema = z.object({ x: z.number(), y: z.number() });
export type ParkPosition = z.infer<typeof parkPositionSchema>;

export const macroCommandSchema = z.enum(["macro1", "macro2", "macro3", "macro4"]);
export type MacroCommand = z.infer<typeof macroCommandSchema>;
/** A macro as the menus show it; `empty` (Rust only) when its gcode is blank, so running it would do nothing. */
export const macroSchema = z.object({
  command: macroCommandSchema,
  title: z.string(),
  empty: z.boolean().default(false),
});

/** What a control-screen button slot holds: an action, the Z-Axis dialog, or a macro. */
export const controlButtonSchema = z.enum([
  "park",
  "zAxis",
  "setHome",
  "setPark",
  ...macroCommandSchema.options,
  "zToZero",
  "zToSafeHeight",
]);
export type ControlButton = z.infer<typeof controlButtonSchema>;
/** The four slots, top to bottom down the jog pad's right side, then beside the distance; never one twice. */
export const controlButtonsSchema = z.array(controlButtonSchema).length(4);
/** PUT /api/display/controls body and answer; applies at once, also during a job. */
export const controlButtonsUpdateSchema = z.strictObject({ buttons: controlButtonsSchema });

/** Server -> client commands for the front page and the shared machine state. */
export const mainMessages = {
  hostAddress: z.object({ hostAddress: z.string() }),
  connectionStatus: z.object({
    status: z.enum(["connected", "disconnected"]),
    port: z.string(),
    fakeServoStatus: z.boolean().optional(),
  }),
  healthMessage: z.object({
    cpuUsage: z.number(),
    bufferSize: z.number(),
    controllerFirmwareVersion: z.number().optional(),
  }),
  statusMessage: z.object({
    uploadFlag: z.number(),
    positioningMode: z.number(),
    currentTool: z.number(),
    /** Set from Play until Stop or the end of the file, through every kind of pause. */
    jobInProgress: z.boolean().optional(),
    /** The job waits for a tool change or a manual Z adjustment. */
    manualZAxisAdjust: z.boolean().optional(),
    /** A G38.2 probe is queued or descending; only Stop, Pause and actions that move nothing run. Rust only. */
    probing: z.boolean().optional(),
    /** The 0-based line a planner build's Stop left, which Play asks whether to resume at. Rust only. */
    stoppedAt: z.number().int().nullable().optional(),
  }),
  positionMessage: z.object({
    xval: z.number(),
    yval: z.number(),
    zval: z.number(),
    vel: z.number(),
    rqvel: z.number(),
    pcom: z.string(),
    state: z.string(),
    lpwr: z.number(),
    rpwr: z.number(),
    zpwr: z.number(),
  }),
  homePositionMessage: z.object({ xval: numeric, yval: numeric }),
  gcodePositionMessage: z.object({
    xval: z.number(),
    yval: z.number(),
    zval: z.number(),
    gcodeLine: z.string(),
    gcodeLineIndex: z.number(),
  }),
  errorValueMessage: z.object({
    leftError: z.number(),
    rightError: z.number(),
    computedX: z.number(),
    computedY: z.number(),
    computedEnabled: z.number(),
  }),
  requestedSetting: z.object({ setting: z.string(), value: z.union([z.string(), z.number(), z.boolean()]) }),
  controllerMessage: z.string(),
  alarm: z.object({ message: z.string() }),
  /** UIProcessor always sends `{ "data": "" }`. */
  clearAlarm: z.object({ data: z.string() }),
  activateModal: modalSchema,
  closeModals: z.object({ title: z.string() }),
  closeAlertModals: z.object({ title: z.string() }),
  /** Broadcast when the loaded gcode file changes: its path in the gcode library, or null once none is loaded. */
  gcodeFile: z.object({ file: z.string().nullable() }),
  pacingStats: pacingStatsSchema,
  /** Broadcast when the park position changes; null while none is set. Rust only. */
  parkPosition: z.object({ position: parkPositionSchema.nullable() }),
  /** Broadcast when a macro's title or gcode changes. Rust only. */
  macros: z.object({ macros: z.array(macroSchema) }),
  /** Broadcast when the control button slots change. Rust only. */
  controlButtons: z.object({ buttons: controlButtonsSchema }),
} satisfies MessageSchemas;

/** processAction commands the front page issues. */
export const mainActions = [
  "move",
  "moveTo",
  "moveZ",
  "stopZ",
  "startRun",
  "pauseRun",
  "resumeRun",
  "stopRun",
  "home",
  "defineHome",
  "resetHomeToCenter",
  "defineZ0",
  "touchZ",
  "zToZero",
  "zToSafeHeight",
  "returnToCenter",
  "park",
  "definePark",
  "macro1",
  "macro2",
  "macro3",
  "macro4",
  "moveGcodeZ",
  "moveGcodeGoto",
  "moveGcodeIndex",
  "setMaxZ",
  "setMinZ",
  "clearZ",
  "getZlimits",
] as const;

export const mainEvents = {
  action: actionEventSchema,
} satisfies EventSchemas;

/** GET /api/state: what a client needs on (re)connect, in one request. */
export const startupStateSchema = z.object({
  units: unitsSchema,
  distToMove: z.number(),
  unitsZ: unitsSchema,
  distToMoveZ: z.number(),
  homePosition: z.object({ x: z.number(), y: z.number() }),
  pauseButtonSetting: z.enum(["Pause", "Resume"]),
  jobInProgress: z.boolean().default(false),
  manualZAxisAdjust: z.boolean().default(false),
  /** A G38.2 probe is queued or descending. Rust only; Python never sends it. */
  probing: z.boolean().default(false),
  hostAddress: z.string(),
  /** The link as `connectionStatus` last pushed it, for a client that missed that push. Absent from Python. */
  connectionStatus: mainMessages.connectionStatus.optional(),
  /** 0 until the controller reports its version on connect. */
  controllerFirmwareVersion: z.number().default(0),
  /** The controller's boot banner, such as "PCB v1.6 TLE9201 Detected"; null until it boots on this connection. */
  controllerBoard: z.string().nullable().default(null),
  /** The version narrowed by fingerprint: every 51.29x build reports 51.29, so "51.292" or "51.29x"; null if unknown. */
  firmwareBuild: z.string().nullable().default(null),
  macros: z.array(macroSchema),
  /** Which backend answered; `wc-switch` reads it. Absent from older Python builds. */
  backend: z.enum(["python", "rust"]).optional(),
  /** The webcontrol-rs release, such as "0.2.0". Rust only; Python never sends it. */
  version: z.string().optional(),
  /** The git commit the webcontrol-rs binary was built from. Rust only. */
  commit: z.string().optional(),
  /** Experimental features turned on in the settings: "optical" for optical calibration. Rust only. */
  experimental: z.array(z.string()).default([]),
  /** The loaded gcode file's path in the gcode library, such as "jobs/leg.nc"; null when none is loaded. */
  gcodeFile: z.string().nullable().default(null),
  /** The line pacing and its statistics as last pushed. Rust only. */
  pacing: pacingStatsSchema.optional(),
  /** The uiTheme setting. Rust only. */
  theme: themeSchema.optional(),
  /** The park position, or null while none is set. Rust only. */
  park: parkPositionSchema.nullable().optional(),
  /** The control button slots. Rust only. */
  controlButtons: controlButtonsSchema.optional(),
});
export type StartupState = z.infer<typeof startupStateSchema>;

/** GET /api/main: the control screen's run status and gcode line, without waiting for the 5 s status push. */
export const mainScreenStateSchema = z.object({
  status: mainMessages.statusMessage,
  touchPlate: z.boolean(),
  gcode: z.object({ lineIndex: z.number(), line: z.string(), lineCount: z.number() }),
});
export type MainScreenState = z.infer<typeof mainScreenStateSchema>;
export type ModalMessage = z.infer<typeof modalSchema>;
export type Units = z.infer<typeof unitsSchema>;
