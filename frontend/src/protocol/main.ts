import { z } from "zod";
import { actionEventSchema, numeric } from "./core";
import type { EventSchemas, MessageSchemas } from "./core";

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
});

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
  statusMessage: z.object({ uploadFlag: z.number(), positioningMode: z.number(), currentTool: z.number() }),
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
  clearAlarm: z.unknown(),
  activateModal: modalSchema,
  closeModals: z.object({ title: z.string() }),
  closeAlertModals: z.object({ title: z.string() }),
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
  "returnToCenter",
  "macro1",
  "macro2",
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
  hostAddress: z.string(),
  /** 0 until the controller reports its version on connect. */
  controllerFirmwareVersion: z.number().default(0),
  macros: z.array(z.object({ command: z.enum(["macro1", "macro2"]), title: z.string() })),
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
