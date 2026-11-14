import { gzip } from "pako";
import { describe, expect, it, vi } from "vitest";
import type { ViewerSnapshot } from "../../protocol";
import { DEFAULT_MACHINE } from "./shapes";
import { initialViewerData, viewerReducer } from "./viewerState";
import type { ViewerEvent } from "./viewerState";

const toBase64 = (bytes: Uint8Array) => btoa(String.fromCharCode(...bytes));
const gz = (value: unknown) => gzip(JSON.stringify(value));
const moves = [
  {
    type: "line",
    points: [
      [0, 0, 0],
      [1, 0, 0],
    ],
    dashed: false,
    command: null,
  },
];
const board = {
  width: 10,
  height: 5,
  thickness: 0.75,
  centerX: 0,
  centerY: 0,
  boardID: "b",
  material: "MDF",
  fileName: "",
};

const snapshot = (over: Partial<ViewerSnapshot> = {}): ViewerSnapshot => ({
  machine: { bedWidth: 96, bedHeight: 48, motorOffsetY: 18.2283, motorSpacingX: 117.2598 },
  gcode: toBase64(gz(moves)),
  board,
  cutData: toBase64(gz([true])),
  ...over,
});

const apply = (...events: ViewerEvent[]) => events.reduce(viewerReducer, initialViewerData);

describe("viewerReducer", () => {
  it("loads the snapshot and stops the spinner", () => {
    const state = apply({ type: "loading" }, { type: "snapshot", snapshot: snapshot() });
    expect(state.loading).toBe(false);
    expect(state.gcode?.cuts).toHaveLength(6);
    expect(state.board).toEqual(board);
    expect(state.cutCells).toEqual([true]);
    expect(state.machine).toBe(initialViewerData.machine);
  });

  it("only replaces the machine when its geometry changes, so the camera is not refit for nothing", () => {
    const bigger = apply({
      type: "snapshot",
      snapshot: snapshot({ machine: { bedWidth: 120, bedHeight: 48, motorOffsetY: 20, motorSpacingX: 140 } }),
    });
    expect(bigger.machine).toEqual({ bedWidth: 120, bedHeight: 48, motorOffsetY: 20, motorSpacingX: 140 });
    expect(bigger.machine).not.toBe(DEFAULT_MACHINE);
  });

  it("shows the spinner once per announcement", () => {
    const loading = apply({ type: "loading" });
    expect(loading.loading).toBe(true);
    expect(viewerReducer(loading, { type: "loading" })).toBe(loading);
  });

  it("replaces and clears the gcode from socket pushes", () => {
    const loaded = apply({ type: "loading" }, { type: "gcode", payload: gz(moves) });
    expect(loaded).toMatchObject({ loading: false, error: null });
    expect(loaded.gcode?.cuts).toHaveLength(6);
    expect(viewerReducer(loaded, { type: "gcode", payload: "" }).gcode).toBeNull();
  });

  it("keeps the last good data and reports corrupt payloads", () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    const loaded = apply({ type: "gcode", payload: gz(moves) }, { type: "loading" });
    const broken = viewerReducer(loaded, { type: "gcode", payload: new Uint8Array([9, 9]) });
    expect(broken).toMatchObject({ loading: false, error: "Could not read the gcode preview from the server." });
    expect(broken.gcode).toBe(loaded.gcode);

    const badCut = viewerReducer(broken, { type: "cutData", payload: "%%%" });
    expect(badCut.error).toBe("Could not read the board cut data from the server.");
    expect(viewerReducer(badCut, { type: "cutData", payload: gz([false]) })).toMatchObject({
      cutCells: [false],
      error: null,
    });
  });

  it("reports a failed snapshot", () => {
    expect(apply({ type: "loading" }, { type: "snapshotFailed" })).toMatchObject({
      loading: false,
      error: "Could not load the preview from the server.",
    });
  });

  it("takes board info from boardDataUpdate", () => {
    expect(apply({ type: "board", board }).board).toEqual(board);
  });

  it("tracks the webcam state and its latest frame", () => {
    const on = apply({ type: "camera", message: { command: "updateCamera", data: "on" } });
    expect(on.camera).toEqual({ on: true, image: null });
    const framed = viewerReducer(on, { type: "camera", message: { command: "cameraImageUpdated", data: "AAA" } });
    expect(framed.camera).toEqual({ on: true, image: "AAA" });
    expect(viewerReducer(framed, { type: "camera", message: { command: "updateCamera", data: "off" } }).camera).toEqual(
      {
        on: false,
        image: "AAA",
      },
    );
  });
});
