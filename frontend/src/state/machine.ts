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
  status: { uploadFlag: number; currentTool: number; positioningMode: number } | null;
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
  status: null,
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
    case "healthMessage":
      return { ...state, health: { cpuUsage: event.data.cpuUsage, bufferSize: event.data.bufferSize } };
    case "statusMessage":
      return { ...state, status: { ...event.data } };
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
      const { units, unitsZ, distToMove, distToMoveZ, homePosition, pauseButtonSetting, hostAddress, macros } =
        event.state;
      return {
        ...state,
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

/** uploadFlag is 1 while gcode streams, 0 when idle, and anything else while a run is paused. */
export const selectRunState = (state: MachineState): RunState => {
  if (!state.status) return "unknown";
  const { uploadFlag } = state.status;
  return uploadFlag === 0 ? "idle" : uploadFlag === 1 ? "running" : "paused";
};

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
