import { describe, expect, it, vi } from "vitest";
import { startupState } from "../test/fakeFetch";
import {
  CONTROLLER_MESSAGE_LIMIT,
  createMachineStore,
  initialMachineState,
  machineReducer,
  selectGcodeLocked,
  selectJobState,
  selectRunState,
} from "./machine";
import type { MachineEvent, MachineState } from "./machine";

const apply = (events: MachineEvent[], from: MachineState = initialMachineState) => events.reduce(machineReducer, from);
const message = <E extends Extract<MachineEvent, { type: "message" }>>(command: E["command"], data: E["data"]) =>
  ({ type: "message", command, data }) as MachineEvent;

describe("machineReducer socket and startup", () => {
  it("tracks the socket and forgets controller and health when it drops", () => {
    const connected = apply([
      { type: "socket", status: "connected" },
      message("connectionStatus", { status: "connected", port: "/dev/ttyACM0", fakeServoStatus: true }),
      message("healthMessage", { cpuUsage: 5, bufferSize: 127 }),
    ]);
    expect(connected.controller).toEqual({ status: "connected", port: "/dev/ttyACM0", fakeServo: true });

    const dropped = machineReducer(connected, { type: "socket", status: "disconnected" });
    expect(dropped).toMatchObject({ socket: "disconnected", controller: initialMachineState.controller, health: null });
  });

  it("returns the same state when the socket status does not change", () => {
    expect(machineReducer(initialMachineState, { type: "socket", status: "connecting" })).toBe(initialMachineState);
  });

  it("loads startup settings from /api/state", () => {
    const state = apply([
      { type: "startup", state: startupState({ units: "INCHES", pauseButtonSetting: "Resume", hostAddress: "h:1" }) },
    ]);
    expect(state).toMatchObject({
      units: "INCHES",
      unitsZ: "MM",
      distToMove: 10,
      distToMoveZ: 1,
      homePosition: { x: 0, y: 0 },
      pauseButton: "Resume",
      hostAddress: "h:1",
      macros: startupState().macros,
    });
  });

  it("knows the loaded gcode file from /api/state, follows gcodeFile pushes and forgets it when cleared", () => {
    expect(initialMachineState.gcodeFile).toBeNull();
    const loaded = apply([{ type: "startup", state: startupState({ gcodeFile: "jobs/leg.nc" }) }]);
    expect(loaded.gcodeFile).toBe("jobs/leg.nc");
    const replaced = machineReducer(loaded, message("gcodeFile", { file: "mancala/top.nc" }));
    expect(replaced.gcodeFile).toBe("mancala/top.nc");
    expect(machineReducer(replaced, message("gcodeFile", { file: null })).gcodeFile).toBeNull();
  });
});

describe("machineReducer park position", () => {
  it("takes the park position from /api/state, follows parkPosition pushes and has none from an older server", () => {
    const loaded = apply([{ type: "startup", state: startupState({ park: { x: 10, y: -20 } }) }]);
    expect(loaded.park).toEqual({ x: 10, y: -20 });
    expect(apply([message("parkPosition", { position: null })], loaded).park).toBeNull();
    expect(apply([{ type: "startup", state: startupState() }], loaded).park).toBeNull();
  });
});

describe("machineReducer link state from /api/state", () => {
  const link = { status: "connected", port: "/dev/ttyACM0", fakeServoStatus: true } as const;

  it("fills in a controller still unknown", () => {
    const state = apply([{ type: "startup", state: startupState({ connectionStatus: link }) }]);
    expect(state.controller).toEqual({ status: "connected", port: "/dev/ttyACM0", fakeServo: true });
  });

  it("leaves a controller a push already reported, since the push may be newer", () => {
    const state = apply([
      message("connectionStatus", { status: "disconnected", port: "none" }),
      { type: "startup", state: startupState({ connectionStatus: link }) },
    ]);
    expect(state.controller).toEqual({ status: "disconnected", port: null, fakeServo: false });
  });

  it("stays unknown when /api/state has no link state", () => {
    expect(apply([{ type: "startup", state: startupState() }]).controller.status).toBe("unknown");
  });
});

describe("machineReducer messages", () => {
  it("records the controller as disconnected without a port", () => {
    const state = apply([message("connectionStatus", { status: "disconnected", port: "none" })]);
    expect(state.controller).toEqual({ status: "disconnected", port: null, fakeServo: false });
  });

  it("stores host address, health and status", () => {
    const state = apply([
      message("hostAddress", { hostAddress: "10.0.0.2:5000" }),
      message("healthMessage", { cpuUsage: 33.3, bufferSize: -1 }),
      message("statusMessage", { uploadFlag: 1, positioningMode: 0, currentTool: 2 }),
    ]);
    expect(state.hostAddress).toBe("10.0.0.2:5000");
    expect(state.health).toEqual({ cpuUsage: 33.3, bufferSize: -1 });
    expect(state.status).toEqual({ uploadFlag: 1, positioningMode: 0, currentTool: 2 });
  });

  it("tracks the firmware version from startup state and health reports", () => {
    const unknown = apply([{ type: "startup", state: startupState({ controllerFirmwareVersion: 0 }) }]);
    expect(unknown.firmwareVersion).toBeNull();

    const loaded = apply([{ type: "startup", state: startupState({ controllerFirmwareVersion: 1.26 }) }]);
    expect(loaded.firmwareVersion).toBe(1.26);

    const reported = apply([
      message("healthMessage", { cpuUsage: 1, bufferSize: -1, controllerFirmwareVersion: 51.27 }),
    ]);
    expect(reported.firmwareVersion).toBe(51.27);
    expect(apply([message("healthMessage", { cpuUsage: 1, bufferSize: -1 })], reported).firmwareVersion).toBe(51.27);
  });

  it("maps sled, home and gcode positions", () => {
    const state = apply([
      message("positionMessage", {
        xval: 1,
        yval: 2,
        zval: -0.5,
        vel: 10,
        rqvel: 20,
        pcom: "12.0%",
        state: "Idle",
        lpwr: 3,
        rpwr: 4,
        zpwr: 5,
      }),
      message("homePositionMessage", { xval: 7, yval: 8 }),
      message("gcodePositionMessage", { xval: 9, yval: 10, zval: 1, gcodeLine: "G1 X9", gcodeLineIndex: 41 }),
    ]);
    expect(state.position).toEqual({
      x: 1,
      y: 2,
      z: -0.5,
      velocity: 10,
      requestedVelocity: 20,
      percentComplete: "12.0%",
      state: "Idle",
      power: { left: 3, right: 4, z: 5 },
    });
    expect(state.homePosition).toEqual({ x: 7, y: 8 });
    expect(state.gcodePosition).toEqual({ x: 9, y: 10, z: 1, line: "G1 X9", lineIndex: 41 });
  });

  it.each([
    [1, 100, { x: 100, y: 200 }],
    [0, 100, null],
    [1, -999999, null],
  ])("keeps the computed position only when enabled (%i, %i)", (computedEnabled, computedX, computed) => {
    const state = apply([
      message("errorValueMessage", { leftError: 0.1, rightError: 0.2, computedX, computedY: 200, computedEnabled }),
    ]);
    expect(state.positionError).toEqual({ left: 0.1, right: 0.2, computed });
  });

  it("applies requested settings and ignores unusable values", () => {
    const state = apply([
      message("requestedSetting", { setting: "units", value: "INCHES" }),
      message("requestedSetting", { setting: "unitsZ", value: "MM" }),
      message("requestedSetting", { setting: "distToMove", value: "2.5" }),
      message("requestedSetting", { setting: "distToMoveZ", value: 0.1 }),
      message("requestedSetting", { setting: "pauseButtonSetting", value: "Resume" }),
      message("requestedSetting", { setting: "units", value: "furlongs" }),
      message("requestedSetting", { setting: "distToMove", value: "far" }),
      message("requestedSetting", { setting: "pauseButtonSetting", value: "Stop" }),
      message("requestedSetting", { setting: "somethingElse", value: 1 }),
    ]);
    expect(state).toMatchObject({
      units: "INCHES",
      unitsZ: "MM",
      distToMove: 2.5,
      distToMoveZ: 0.1,
      pauseButton: "Resume",
    });
  });

  it("raises and clears alarms", () => {
    const raised = apply([message("alarm", { message: "Alarm: Position Lost.  Reset Chains." })]);
    expect(raised.alarm).toBe("Alarm: Position Lost.  Reset Chains.");
    expect(machineReducer(raised, message("clearAlarm", { data: "" })).alarm).toBeNull();
    expect(machineReducer(initialMachineState, message("clearAlarm", { data: "" }))).toBe(initialMachineState);
  });

  it("keeps the last 100 controller messages", () => {
    const events = Array.from({ length: CONTROLLER_MESSAGE_LIMIT + 5 }, (_, i) =>
      message("controllerMessage", `m${i}`),
    );
    const { controllerMessages } = apply(events);
    expect(controllerMessages).toHaveLength(CONTROLLER_MESSAGE_LIMIT);
    expect(controllerMessages[0]).toBe("m5");
    expect(controllerMessages.at(-1)).toBe(`m${CONTROLLER_MESSAGE_LIMIT + 4}`);
  });
});

describe("selectRunState", () => {
  const withFlag = (uploadFlag: number) =>
    apply([message("statusMessage", { uploadFlag, positioningMode: 0, currentTool: 0 })]);

  it.each([
    [0, "idle"],
    [1, "running"],
    [2, "paused"],
    [-1, "paused"],
  ])("uploadFlag %i is %s", (flag, runState) => {
    expect(selectRunState(withFlag(flag))).toBe(runState);
  });

  it("is unknown before the first status message", () => {
    expect(selectRunState(initialMachineState)).toBe("unknown");
  });
});

describe("job state", () => {
  const status = (
    uploadFlag: number,
    job: { jobInProgress?: boolean; manualZAxisAdjust?: boolean; probing?: boolean } = {},
  ) => message("statusMessage", { uploadFlag, positioningMode: 0, currentTool: 0, ...job });

  it("counts a job paused by the controller as paused, not idle", () => {
    const state = apply([status(0, { jobInProgress: true })]);
    expect(selectRunState(state)).toBe("paused");
    expect(selectJobState(state)).toBe("Paused");
    expect(selectGcodeLocked(state)).toBe(true);
  });

  it("reads running, idle and a tool change wait", () => {
    expect(selectJobState(apply([status(1, { jobInProgress: true })]))).toBe("Running");
    expect(selectJobState(apply([status(0)]))).toBe("Idle");
    expect(selectJobState(apply([status(-1, { jobInProgress: true, manualZAxisAdjust: true })]))).toBe(
      "Waiting for tool change",
    );
    expect(selectJobState(initialMachineState)).toBeNull();
  });

  it("keeps the last job flags when a status message leaves them out", () => {
    const state = apply([status(0, { jobInProgress: true, manualZAxisAdjust: true, probing: true }), status(0)]);
    expect(state.job).toEqual({ inProgress: true, manualZAxisAdjust: true, probing: true });
  });

  it("locks gcode while uploading, adjusting Z or in a job, and not when idle or unknown", () => {
    expect(selectGcodeLocked(initialMachineState)).toBe(false);
    expect(selectGcodeLocked(apply([status(0)]))).toBe(false);
    expect(selectGcodeLocked(apply([status(2)]))).toBe(true);
    expect(selectGcodeLocked(apply([status(0, { manualZAxisAdjust: true })]))).toBe(true);
  });

  it("loads the job flags from /api/state", () => {
    const state = apply([{ type: "startup", state: startupState({ jobInProgress: true, manualZAxisAdjust: true }) }]);
    expect(state.job).toEqual({ inProgress: true, manualZAxisAdjust: true, probing: false });
    const probing = apply([{ type: "startup", state: startupState({ probing: true }) }]);
    expect(probing.job.probing).toBe(true);
  });

  it("reads Probing while a probe runs, and ends it when the status says so", () => {
    const probing = apply([status(0, { probing: true })]);
    expect(selectJobState(probing)).toBe("Probing");
    expect(selectJobState(apply([status(0, { probing: false })], probing))).toBe("Idle");
  });
});

describe("createMachineStore", () => {
  it("notifies subscribers only when state changes", () => {
    const store = createMachineStore();
    const listener = vi.fn();
    const unsubscribe = store.subscribe(listener);

    store.dispatch({ type: "socket", status: "connecting" });
    expect(listener).not.toHaveBeenCalled();

    store.dispatch({ type: "socket", status: "connected" });
    expect(listener).toHaveBeenCalledTimes(1);
    expect(store.getState().socket).toBe("connected");

    unsubscribe();
    store.dispatch({ type: "socket", status: "disconnected" });
    expect(listener).toHaveBeenCalledTimes(1);
  });
});
