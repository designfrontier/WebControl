import { act, fireEvent, screen, within } from "@testing-library/react";
import { gzip } from "pako";
import type { Object3D, Scene } from "three";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ViewerSnapshot } from "../../protocol";
import { fakeFetch, jsonResponse } from "../../test/fakeFetch";
import { topLayer } from "../../test/dialogPolyfill";
import { renderWithProviders } from "../../test/render";
import type { RenderOptions } from "../../test/render";
import { LONG_PRESS_MS, Viewer } from "./Viewer";

const engine = vi.hoisted(() => ({
  instances: [] as { root: Scene; host: HTMLElement; [method: string]: unknown }[],
  pickResult: { x: 12.345678, y: -6.5 } as { x: number; y: number } | null,
  fail: false,
}));

vi.mock("./engine", () => ({
  createViewerEngine: (host: HTMLElement, root: Scene) => {
    if (engine.fail) throw new Error("no WebGL");
    const instance = {
      host,
      root,
      requestRender: vi.fn(),
      pick: vi.fn(() => engine.pickResult),
      setRotate: vi.fn(),
      setPerspective: vi.fn(),
      fit: vi.fn(),
      reset: vi.fn(),
      dispose: vi.fn(),
    };
    engine.instances.push(instance);
    return instance;
  },
}));

const gz = (value: unknown) => gzip(JSON.stringify(value));
const toBase64 = (bytes: Uint8Array) => btoa(String.fromCharCode(...bytes));
const asArrayBuffer = (bytes: Uint8Array) => bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);

const line = (points: [number, number, number][], dashed = false) => ({ type: "line", points, dashed, command: null });
const board = {
  width: 24,
  height: 12,
  thickness: 0.75,
  centerX: 0,
  centerY: 0,
  boardID: "Shelf",
  material: "Birch ply",
  fileName: "",
};
const snapshot = (over: Partial<ViewerSnapshot> = {}): ViewerSnapshot => ({
  machine: { bedWidth: 96, bedHeight: 48, motorOffsetY: 18.2283, motorSpacingX: 117.2598 },
  gcode: toBase64(
    gz([
      line([
        [0, 0, 0],
        [1, 0, -0.1],
      ]),
    ]),
  ),
  board,
  cutData: null,
  ...over,
});

const current = () => engine.instances.at(-1)!;
const object = (name: string) => current().root.getObjectByName(name) as Object3D;
const canvas = () => screen.getByRole("img", { name: /Bed, sled and toolpath/ });

const renderViewer = (options: RenderOptions & { viewer?: ViewerSnapshot } = {}) =>
  renderWithProviders(<Viewer />, { fetch: fakeFetch({ "/api/viewer": options.viewer ?? snapshot() }), ...options });

const loaded = async (options?: Parameters<typeof renderViewer>[0]) => {
  const result = renderViewer(options);
  await screen.findByText("1 cut and 0 rapid moves");
  return result;
};

beforeEach(() => {
  engine.instances.length = 0;
  engine.pickResult = { x: 12.345678, y: -6.5 };
  engine.fail = false;
});

describe("Viewer loading", () => {
  it("loads gcode, board and machine from /api/viewer and asks for the camera status", async () => {
    const { socket } = await loaded();
    expect(screen.getByText("Board: Shelf, Material: Birch ply")).toBeInTheDocument();
    expect(current().fit).toHaveBeenLastCalledWith({ x: 0, y: 0, width: 96, height: 48 });
    expect(socket.emittedEvents("action")).toContainEqual({
      data: { command: "statusRequest", arg: "cameraStatus", arg1: "" },
    });
    expect(screen.getByRole("figure", { name: "Gcode and board viewer" })).toHaveAttribute("data-cut-segments", "1");
  });

  it("shows the spinner while the server streams gcode, then draws the binary socket payload", async () => {
    const { socket } = await loaded();
    socket.serverMessage("showFPSpinner", 2048);
    expect(screen.getByText("Loading preview…")).toHaveAttribute("role", "status");

    const moves = [
      line([
        [0, 0, 0],
        [1, 1, 0],
        [2, 2, 0],
      ]),
      line(
        [
          [2, 2, 0],
          [0, 0, 1],
        ],
        true,
      ),
    ];
    socket.serverRaw({ command: "gcodeUpdateCompressed", data: asArrayBuffer(gz(moves)), dataFormat: "base64" });

    expect(screen.queryByText("Loading preview…")).not.toBeInTheDocument();
    expect(screen.getByText("2 cut and 1 rapid moves")).toBeInTheDocument();
    socket.serverRaw({ command: "gcodeUpdateCompressed", data: "", dataFormat: "base64" });
    expect(screen.getByText("No gcode loaded")).toBeInTheDocument();
  });

  it("updates the board from boardDataUpdate and cut data pushes", async () => {
    const { socket } = await loaded();
    socket.serverMessage("boardDataUpdate", { ...board, boardID: "Door", material: "Oak" });
    expect(screen.getByText("Board: Door, Material: Oak")).toBeInTheDocument();
    socket.serverRaw({
      command: "boardCutDataUpdateCompressed",
      data: asArrayBuffer(gz([true])),
      dataFormat: "base64",
    });
    const cells = object("cutCells") as unknown as { geometry: { getAttribute(name: string): { count: number } } };
    expect(cells.geometry.getAttribute("position").count).toBe(6);
  });

  it("reports a failed snapshot", async () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    renderViewer({ fetch: fakeFetch({ "/api/viewer": () => jsonResponse({}, 500) }) });
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not load the preview from the server.");
  });

  it("keeps the old drawing when a push is corrupt", async () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    const { socket } = await loaded();
    socket.serverRaw({ command: "gcodeUpdateCompressed", data: new ArrayBuffer(4), dataFormat: "base64" });
    expect(screen.getByRole("alert")).toHaveTextContent("Could not read the gcode preview from the server.");
    expect(screen.getByText("1 cut and 0 rapid moves")).toBeInTheDocument();
  });

  it("keeps gcode pushed while /api/viewer was loading instead of the older snapshot", async () => {
    const pending: ((response: Response) => void)[] = [];
    const fetch = fakeFetch({ "/api/viewer": () => new Promise<Response>((done) => pending.push(done)) });
    const { socket } = renderViewer({ fetch });
    socket.serverRaw({ command: "gcodeUpdateCompressed", data: asArrayBuffer(gz([])), dataFormat: "base64" });
    expect(screen.getByText("0 cut and 0 rapid moves")).toBeInTheDocument();

    await act(async () => pending.forEach((done) => done(jsonResponse(snapshot()))));
    expect(screen.getByText("Board: Shelf, Material: Birch ply")).toBeInTheDocument();
    expect(screen.getByText("0 cut and 0 rapid moves")).toBeInTheDocument();
  });

  it("explains when WebGL cannot start", async () => {
    engine.fail = true;
    renderViewer();
    expect(screen.getByText(/needs WebGL/)).toBeInTheDocument();
    expect(screen.queryByRole("img")).not.toBeInTheDocument();
    expect(await screen.findByText("1 cut and 0 rapid moves")).toBeInTheDocument();
  });

  it("disposes the engine when unmounted", async () => {
    const { unmount } = await loaded();
    const last = current();
    unmount();
    expect(last.dispose).toHaveBeenCalled();
  });
});

describe("Viewer positions", () => {
  it("draws the sled, home and gcode position in scene inches", async () => {
    const { socket } = await loaded();
    socket.serverMessage("requestedSetting", { setting: "units", value: "MM" });
    socket.serverMessage("positionMessage", {
      xval: 254,
      yval: -25.4,
      zval: 0,
      vel: 0,
      rqvel: 0,
      pcom: "0%",
      state: "Idle",
      lpwr: 0,
      rpwr: 0,
      zpwr: 0,
    });
    socket.serverMessage("homePositionMessage", { xval: 50.8, yval: 25.4 });
    socket.serverMessage("gcodePositionMessage", { xval: 25.4, yval: 0, zval: 0, gcodeLine: "G1", gcodeLineIndex: 3 });
    socket.serverMessage("errorValueMessage", {
      leftError: 0,
      rightError: 0,
      computedX: 127,
      computedY: 0,
      computedEnabled: 1,
    });

    expect(object("sled").position.toArray()).toEqual([10, -1, 0]);
    expect(object("home").position.toArray()).toEqual([2, 1, 0]);
    expect(object("toolpath").position.toArray()).toEqual([2, 1, 0]);
    expect(object("gcodePosition").position.toArray()).toEqual([1, 0, 0]);
    expect(object("computedSled").position.x).toBe(5);
    expect(current().requestRender).toHaveBeenCalled();
  });

  it("shows the cursor position in machine units and clears it when the pointer leaves", async () => {
    await loaded({ machine: { units: "MM" } });
    fireEvent.pointerMove(canvas(), { pointerType: "mouse", clientX: 5, clientY: 5 });
    const readout = screen.getByRole("status", { name: "Cursor position" });
    expect(readout).toHaveTextContent("X: 313.58, Y: -165.10 mm");
    expect(object("cursor").visible).toBe(true);
    fireEvent.pointerLeave(canvas());
    expect(readout).toHaveTextContent("X: –, Y: –");
  });
});

describe("Viewer toolbar", () => {
  it("toggles 3D, perspective, board and labels, and resets the view", async () => {
    const { user } = await loaded();
    const toolbar = screen.getByRole("toolbar", { name: "View" });
    const button = (name: string) => within(toolbar).getByRole("button", { name });

    await user.click(button("3D"));
    expect(button("3D")).toHaveAttribute("aria-pressed", "true");
    expect(current().setRotate).toHaveBeenLastCalledWith(true);

    await user.click(button("Perspective"));
    expect(current().setPerspective).toHaveBeenLastCalledWith(true);

    await user.click(button("Reset view"));
    expect(current().reset).toHaveBeenCalled();

    expect(object("boardLayer").visible).toBe(false);
    await user.click(within(toolbar).getByRole("button", { name: /^Board: Shelf/ }));
    expect(object("boardLayer").visible).toBe(true);

    await user.click(button("Labels"));
    expect(button("Labels")).toHaveAttribute("aria-pressed", "true");
  });

  it("shows webcam frames once decoded while the camera is on", async () => {
    const { socket } = await loaded();
    socket.serverMessage("cameraMessage", { command: "updateCamera", data: "on" });
    socket.serverMessage("cameraMessage", { command: "cameraImageUpdated", data: "iVBORw0KGgo=" });
    expect(screen.queryByRole("img", { name: "Webcam view of the machine" })).not.toBeInTheDocument();

    const pending = document.querySelector(".viewer-camera-next")!;
    fireEvent.load(pending);
    expect(screen.getByRole("img", { name: "Webcam view of the machine" })).toHaveAttribute(
      "src",
      "data:image/png;base64,iVBORw0KGgo=",
    );
    expect(document.querySelector(".viewer-camera-next")).toBeNull();

    socket.serverMessage("cameraMessage", { command: "updateCamera", data: "off" });
    expect(screen.queryByRole("img", { name: "Webcam view of the machine" })).not.toBeInTheDocument();

    socket.serverMessage("cameraMessage", { command: "updateCamera", data: "on" });
    socket.serverMessage("cameraMessage", { command: "cameraImageUpdated", data: "AAAA" });
    expect(screen.queryByRole("img", { name: "Webcam view of the machine" })).not.toBeInTheDocument();
  });
});

describe("Viewer screen actions", () => {
  it("right-click opens move/home actions that send inches rounded to 4 places", async () => {
    const { user, socket } = await loaded({ machine: { units: "MM" } });
    fireEvent.pointerDown(canvas(), { pointerType: "mouse", button: 2 });
    fireEvent.contextMenu(canvas(), { clientX: 30, clientY: 40 });

    const dialog = screen.getByRole("dialog", { name: "Screen action" });
    expect(current().pick).toHaveBeenLastCalledWith(30, 40);
    expect(within(dialog).getByText("X 313.6, Y -165.1 mm")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();

    await user.click(within(dialog).getByRole("button", { name: "Move here" }));
    expect(socket.actions()).toContainEqual(["moveTo", 12.3457, -6.5]);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    fireEvent.contextMenu(canvas());
    await user.click(screen.getByRole("button", { name: "Define home here" }));
    expect(socket.actions()).toContainEqual(["defineHome", 12.3457, -6.5]);

    fireEvent.contextMenu(canvas());
    await user.click(screen.getByRole("button", { name: "Return to center" }));
    expect(socket.actions()).toContainEqual(["returnToCenter", "", ""]);
  });

  it("closes without sending on Cancel, Escape or a click outside", async () => {
    const { user, socket } = await loaded();
    socket.serverMessage("requestedSetting", { setting: "units", value: "INCHES" });
    socket.clearEmitted();
    fireEvent.contextMenu(canvas());
    expect(screen.getByText("X 12.35, Y -6.50 in")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    fireEvent.contextMenu(canvas());
    await user.keyboard("{Escape}");
    fireEvent.contextMenu(canvas());
    await user.click(screen.getByRole("dialog"));
    fireEvent.contextMenu(canvas());
    await user.click(screen.getByText(/^X /));
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(socket.actions()).toEqual([]);
  });

  it("does nothing on right-click in 3D, where the right button rotates, or off the bed plane", async () => {
    const { user } = await loaded();
    engine.pickResult = null;
    fireEvent.contextMenu(canvas());
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    engine.pickResult = { x: 0, y: 0 };
    await user.click(screen.getByRole("button", { name: "3D" }));
    fireEvent.contextMenu(canvas());
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("will not move off the work area", async () => {
    engine.pickResult = { x: 60, y: 0 };
    await loaded({ machine: { status: { uploadFlag: 0, currentTool: 0, positioningMode: 0 } } });
    fireEvent.contextMenu(canvas());
    expect(screen.getByRole("button", { name: "Move here" })).toBeDisabled();
    expect(screen.getByText(/outside the work area/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Define home here" })).toBeEnabled();
  });

  it("disables the actions while gcode is running", async () => {
    await loaded({ machine: { status: { uploadFlag: 1, currentTool: 0, positioningMode: 0 } } });
    fireEvent.contextMenu(canvas());
    expect(screen.getByText("Unavailable while gcode is running.")).toBeInTheDocument();
    ["Move here", "Define home here", "Return to center"].forEach((name) =>
      expect(screen.getByRole("button", { name })).toBeDisabled(),
    );
  });

  it("disables only Define home while a run is paused, which the server refuses", async () => {
    await loaded({ machine: { status: { uploadFlag: 2, currentTool: 0, positioningMode: 0 } } });
    fireEvent.contextMenu(canvas());
    expect(screen.getByText("Define home is unavailable while a run is paused.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Define home here" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Move here" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Return to center" })).toBeEnabled();
  });

  it("opens in the top layer and gives focus back on close", async () => {
    const { user } = await loaded();
    const threeD = screen.getByRole("button", { name: "3D" });
    threeD.focus();
    fireEvent.contextMenu(canvas());
    const dialog = screen.getByRole("dialog");
    expect(topLayer()).toEqual([dialog]);
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();

    await user.keyboard("{Escape}");
    expect(topLayer()).toEqual([]);
    expect(threeD).toHaveFocus();
  });

  it("shows the current gcode line and a placeholder without one", () => {
    const { unmount } = renderViewer();
    expect(screen.getByRole("status", { name: "Current gcode line" })).toHaveTextContent("No line");
    unmount();
    renderWithProviders(<Viewer currentLine="G1 X2" />, { fetch: fakeFetch({ "/api/viewer": snapshot() }) });
    expect(screen.getByRole("status", { name: "Current gcode line" })).toHaveTextContent("G1 X2");
  });

  it("frames toolpath that runs off the bed", async () => {
    const offBed = toBase64(
      gz([
        line([
          [0, 0, 0],
          [70, 0, 0],
        ]),
      ]),
    );
    renderViewer({ viewer: snapshot({ gcode: offBed }) });
    await screen.findByText("1 cut and 0 rapid moves");
    expect(current().fit).toHaveBeenLastCalledWith({ x: 11, y: 0, width: 118, height: 48 });
  });

  it("recolours the scene from the theme's tokens", async () => {
    document.documentElement.style.setProperty("--scene-background", "#eee");
    await loaded();
    expect((current().root.background as { getHex(): number }).getHex()).toBe(0xeeeeee);
    document.documentElement.style.removeProperty("--scene-background");
  });

  it("does not start a long press for the second finger of a pinch", async () => {
    await loaded();
    vi.useFakeTimers();
    fireEvent.pointerDown(canvas(), { pointerType: "touch", isPrimary: true, clientX: 50, clientY: 60 });
    fireEvent.pointerDown(canvas(), { pointerType: "touch", isPrimary: false, clientX: 90, clientY: 60 });
    act(() => vi.advanceTimersByTime(LONG_PRESS_MS));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("opens on a 1.5 s long press and ignores the touch contextmenu event", async () => {
    await loaded();
    vi.useFakeTimers();
    fireEvent.pointerDown(canvas(), { pointerType: "touch", isPrimary: true, clientX: 50, clientY: 60 });
    fireEvent.contextMenu(canvas());
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    act(() => vi.advanceTimersByTime(LONG_PRESS_MS - 1));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    act(() => vi.advanceTimersByTime(1));
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(current().pick).toHaveBeenLastCalledWith(50, 60);
  });

  it("cancels a long press when the finger lifts or drags", async () => {
    await loaded();
    vi.useFakeTimers();
    fireEvent.pointerDown(canvas(), { pointerType: "touch", isPrimary: true, clientX: 50, clientY: 60 });
    fireEvent.pointerMove(canvas(), { pointerType: "touch", clientX: 52, clientY: 62 });
    fireEvent.pointerMove(canvas(), { pointerType: "touch", clientX: 80, clientY: 60 });
    act(() => vi.advanceTimersByTime(LONG_PRESS_MS));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    fireEvent.pointerDown(canvas(), { pointerType: "touch", isPrimary: true, clientX: 50, clientY: 60 });
    fireEvent.pointerUp(canvas(), { pointerType: "touch" });
    act(() => vi.advanceTimersByTime(LONG_PRESS_MS));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});
