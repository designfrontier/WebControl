import type { MessageData, StartupState, Units } from "../protocol";
import type { ConnectionStatus } from "../socket/client";

export interface Point {
  x: number;
  y: number;
}

export interface MachineState {
  socket: ConnectionStatus;
  hostAddress: string | null;
  controller: { status: "unknown" | "connected" | "disconnected"; port: string | null; fakeServo: boolean };
  health: { cpuUsage: number; bufferSize: number } | null;
  /** The controller's firmware version, or null until it reports one. */
  firmwareVersion: number | null;
  status: { uploadFlag: number; currentTool: number; positioningMode: number } | null;
  /** Whether a job is running or paused, which uploadFlag alone loses on controller pauses; probing is Rust only. */
  job: { inProgress: boolean; manualZAxisAdjust: boolean; probing: boolean };
  position:
    | (Point & {
        z: number;
        velocity: number;
        requestedVelocity: number;
        percentComplete: string;
        state: string;
        power: { left: number; right: number; z: number };
      })
    | null;
  positionError: { left: number; right: number; computed: Point | null } | null;
  homePosition: Point | null;
  gcodePosition: (Point & { z: number; line: string; lineIndex: number }) | null;
  units: Units | null;
  unitsZ: Units | null;
  distToMove: number | null;
  distToMoveZ: number | null;
  pauseButton: "Pause" | "Resume";
  macros: StartupState["macros"];
  alarm: string | null;
  controllerMessages: string[];
}

/** Server commands the store folds into MachineState. */
export const STORE_COMMANDS = [
  "hostAddress",
  "connectionStatus",
  "healthMessage",
  "statusMessage",
  "positionMessage",
  "errorValueMessage",
  "homePositionMessage",
  "gcodePositionMessage",
  "requestedSetting",
  "controllerMessage",
  "alarm",
  "clearAlarm",
] as const;
export type StoreCommand = (typeof STORE_COMMANDS)[number];

export type MachineEvent =
  | { type: "socket"; status: ConnectionStatus }
  | { type: "startup"; state: StartupState }
  | { [C in StoreCommand]: { type: "message"; command: C; data: MessageData<C> } }[StoreCommand];

export const CONTROLLER_MESSAGE_LIMIT = 100;
/** UIProcessor sends -999999 for the computed position when forward kinematics are off. */
const NO_COMPUTED_POSITION = -999999;

export const initialMachineState: MachineState = {
  socket: "connecting",
  hostAddress: null,
  controller: { status: "unknown", port: null, fakeServo: false },
  health: null,
  firmwareVersion: null,
  status: null,
  job: { inProgress: false, manualZAxisAdjust: false, probing: false },
  position: null,
  positionError: null,
  homePosition: null,
  gcodePosition: null,
  units: null,
  unitsZ: null,
  distToMove: null,
  distToMoveZ: null,
  pauseButton: "Pause",
  macros: [],
  alarm: null,
  controllerMessages: [],
};

const reportedVersion = (version: number) => (version > 0 ? version : null);

const asUnits = (value: unknown): Units | null => (value === "MM" || value === "INCHES" ? value : null);
const asNumber = (value: unknown) => (Number.isFinite(Number(value)) ? Number(value) : null);

const applySetting = (state: MachineState, { setting, value }: MessageData<"requestedSetting">): MachineState => {
  switch (setting) {
    case "units":
    case "unitsZ":
      return { ...state, [setting]: asUnits(value) ?? state[setting] };
    case "distToMove":
    case "distToMoveZ":
      return { ...state, [setting]: asNumber(value) ?? state[setting] };
    case "pauseButtonSetting":
      return value === "Pause" || value === "Resume" ? { ...state, pauseButton: value } : state;
    default:
      return state;
  }
};

const applyMessage = (state: MachineState, event: Extract<MachineEvent, { type: "message" }>): MachineState => {
  switch (event.command) {
    case "hostAddress":
      return { ...state, hostAddress: event.data.hostAddress };
    case "connectionStatus":
      return {
        ...state,
        controller: {
          status: event.data.status,
          port: event.data.status === "connected" ? event.data.port : null,
          fakeServo: event.data.fakeServoStatus ?? false,
        },
      };
    case "healthMessage": {
      const { cpuUsage, bufferSize, controllerFirmwareVersion } = event.data;
      return {
        ...state,
        health: { cpuUsage, bufferSize },
        firmwareVersion:
          controllerFirmwareVersion === undefined ? state.firmwareVersion : reportedVersion(controllerFirmwareVersion),
      };
    }
    case "statusMessage": {
      const { uploadFlag, positioningMode, currentTool, jobInProgress, manualZAxisAdjust, probing } = event.data;
      return {
        ...state,
        status: { uploadFlag, positioningMode, currentTool },
        job: {
          inProgress: jobInProgress ?? state.job.inProgress,
          manualZAxisAdjust: manualZAxisAdjust ?? state.job.manualZAxisAdjust,
          probing: probing ?? state.job.probing,
        },
      };
    }
    case "positionMessage": {
      const { xval, yval, zval, vel, rqvel, pcom, state: machineState, lpwr, rpwr, zpwr } = event.data;
      return {
        ...state,
        position: {
          x: xval,
          y: yval,
          z: zval,
          velocity: vel,
          requestedVelocity: rqvel,
          percentComplete: pcom,
          state: machineState,
          power: { left: lpwr, right: rpwr, z: zpwr },
        },
      };
    }
    case "errorValueMessage": {
      const { leftError, rightError, computedX, computedY, computedEnabled } = event.data;
      const computed = computedEnabled && computedX !== NO_COMPUTED_POSITION ? { x: computedX, y: computedY } : null;
      return { ...state, positionError: { left: leftError, right: rightError, computed } };
    }
    case "homePositionMessage":
      return { ...state, homePosition: { x: event.data.xval, y: event.data.yval } };
    case "gcodePositionMessage": {
      const { xval, yval, zval, gcodeLine, gcodeLineIndex } = event.data;
      return { ...state, gcodePosition: { x: xval, y: yval, z: zval, line: gcodeLine, lineIndex: gcodeLineIndex } };
    }
    case "requestedSetting":
      return applySetting(state, event.data);
    case "controllerMessage":
      return {
        ...state,
        controllerMessages: [...state.controllerMessages, event.data].slice(-CONTROLLER_MESSAGE_LIMIT),
      };
    case "alarm":
      return { ...state, alarm: event.data.message };
    case "clearAlarm":
      return state.alarm === null ? state : { ...state, alarm: null };
  }
};

export const machineReducer = (state: MachineState, event: MachineEvent): MachineState => {
  switch (event.type) {
    case "socket":
      if (event.status === state.socket) return state;
      return event.status === "connected"
        ? { ...state, socket: event.status }
        : { ...state, socket: event.status, controller: initialMachineState.controller, health: null };
    case "startup": {
      const {
        units,
        unitsZ,
        distToMove,
        distToMoveZ,
        homePosition,
        pauseButtonSetting,
        hostAddress,
        macros,
        controllerFirmwareVersion,
        jobInProgress,
        manualZAxisAdjust,
        probing,
      } = event.state;
      return {
        ...state,
        job: { inProgress: jobInProgress, manualZAxisAdjust, probing },
        firmwareVersion: reportedVersion(controllerFirmwareVersion),
        units,
        unitsZ,
        distToMove,
        distToMoveZ,
        homePosition,
        pauseButton: pauseButtonSetting,
        hostAddress,
        macros,
      };
    }
    case "message":
      return applyMessage(state, event);
  }
};

export type RunState = "unknown" | "idle" | "running" | "paused";

/**
 * uploadFlag is 1 while gcode streams and anything but 0 while a run is paused; a controller pause drops it to 0,
 * so a job still in progress counts as paused.
 */
export const selectRunState = (state: MachineState): RunState => {
  if (!state.status) return "unknown";
  const { uploadFlag } = state.status;
  return uploadFlag === 1 ? "running" : uploadFlag !== 0 || state.job.inProgress ? "paused" : "idle";
};

export type JobState = "Idle" | "Running" | "Paused" | "Waiting for tool change" | "Probing";

export const selectJobState = (state: MachineState): JobState | null => {
  const run = selectRunState(state);
  if (run === "unknown") return null;
  if (state.job.probing) return "Probing";
  if (state.job.manualZAxisAdjust) return "Waiting for tool change";
  return run === "running" ? "Running" : run === "paused" ? "Paused" : "Idle";
};

/** Mirrors the server's gcodeLocked(): no loading, clearing or editing gcode until the job is stopped. */
export const selectGcodeLocked = (state: MachineState) =>
  state.job.inProgress || state.job.manualZAxisAdjust || (state.status !== null && state.status.uploadFlag !== 0);

/** A G38.2 is queued or descending: the server refuses every move until it answers or Stop. */
export const selectProbing = (state: MachineState) => state.job.probing;

export interface MachineStore {
  getState(): MachineState;
  dispatch(event: MachineEvent): void;
  subscribe(listener: () => void): () => void;
}

export const createMachineStore = (initial: MachineState = initialMachineState): MachineStore => {
  let state = initial;
  const listeners = new Set<() => void>();
  return {
    getState: () => state,
    dispatch: (event) => {
      const next = machineReducer(state, event);
      if (next === state) return;
      state = next;
      listeners.forEach((listener) => listener());
    },
    subscribe: (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
};
