import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { MainScreenState } from "../../protocol";
import { fakeFetch, jsonResponse } from "../../test/fakeFetch";
import type { FakeSocket } from "../../test/fakeSocket";
import { renderApp } from "../../test/render";
import type { RenderOptions } from "../../test/render";

const mainState = (over: Partial<MainScreenState> = {}): MainScreenState => ({
  status: { uploadFlag: 0, positioningMode: 0, currentTool: 1 },
  touchPlate: false,
  gcode: { lineIndex: 4, line: "G1 X10 Y5", lineCount: 120 },
  ...over,
});

const status = (socket: FakeSocket, uploadFlag: number) =>
  socket.serverMessage("statusMessage", { uploadFlag, positioningMode: 0, currentTool: 2 });

/** Renders the control screen with GET /api/main answered, and waits for the run status it seeds. */
const renderMain = async (state = mainState(), options: RenderOptions = {}) => {
  const view = renderApp("/", { fetch: fakeFetch({ "/api/main": state }), ...options });
  await waitFor(() => expect(screen.getByRole("region", { name: "Jog" })).toBeInTheDocument());
  await waitFor(() => expect(view.store.getState().status).not.toBeNull());
  view.socket.clearEmitted();
  return view;
};

const button = (name: string | RegExp) => screen.getByRole("button", { name });

describe("MainPage run status", () => {
  it("keeps every control but Stop disabled until the run status is known", () => {
    renderApp("/");
    ["Play", "Pause", "Up", "Home", "Set Home Here", "Next line"].forEach((name) =>
      expect(button(name)).toBeDisabled(),
    );
    expect(button("Stop")).toBeEnabled();
  });

  it("seeds the run status from GET /api/main on connect", async () => {
    await renderMain(mainState({ status: { uploadFlag: 0, positioningMode: 1, currentTool: 3 } }));
    expect(button("Play")).toBeEnabled();
    expect(screen.getByText("Incremental (G91)")).toBeInTheDocument();
    expect(screen.getByText("G1 X10 Y5")).toBeInTheDocument();
    expect(screen.getByLabelText("Line #")).toHaveValue(5);
  });

  it("tolerates a failed /api/main and waits for the status push", async () => {
    const { socket } = renderApp("/", { fetch: fakeFetch({ "/api/main": () => jsonResponse({}, 500) }) });
    await waitFor(() => expect(screen.getByText("Server: connected (192.168.1.20:5000)")).toBeInTheDocument());
    expect(button("Play")).toBeDisabled();
    status(socket, 0);
    expect(button("Play")).toBeEnabled();
  });

  it("ignores a /api/main answer that lands after the socket dropped", async () => {
    let answer: (response: Response) => void = () => {};
    const fetch = fakeFetch({ "/api/main": () => new Promise<Response>((resolve) => (answer = resolve)) });
    const { socket, store } = renderApp("/", { fetch });
    await waitFor(() => expect(fetch).toHaveBeenCalledWith("/api/main", expect.anything()));
    socket.serverDisconnect();
    answer(jsonResponse(mainState()));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(store.getState().status).toBeNull();
  });

  it("keeps a status pushed while GET /api/main was in flight", async () => {
    let answer: (response: Response) => void = () => {};
    const fetch = fakeFetch({ "/api/main": () => new Promise<Response>((resolve) => (answer = resolve)) });
    const { socket, store } = renderApp("/", { fetch });
    await waitFor(() => expect(fetch).toHaveBeenCalledWith("/api/main", expect.anything()));
    status(socket, 1);
    answer(jsonResponse(mainState()));
    await waitFor(() => expect(screen.getByText("G1 X10 Y5")).toBeInTheDocument());
    expect(store.getState().status?.uploadFlag).toBe(1);
    expect(button("Play")).toBeDisabled();
  });

  it("gates controls while running: only Pause and Stop work", async () => {
    const { socket } = await renderMain();
    status(socket, 1);
    expect(button("Play")).toBeDisabled();
    expect(button("Pause")).toBeEnabled();
    expect(button("Up")).toBeDisabled();
    expect(button("Macro 1")).toBeDisabled();
    expect(button("Next line")).toBeDisabled();
    expect(button("Units: mm")).toBeDisabled();
    expect(button("Stop")).toBeEnabled();
  });

  it("allows jogging and stepping while paused, and Pause becomes Resume", async () => {
    const { socket, user } = await renderMain();
    status(socket, 2);
    socket.serverMessage("requestedSetting", { setting: "pauseButtonSetting", value: "Resume" });
    expect(button("Play")).toBeDisabled();
    expect(button("Up")).toBeEnabled();
    expect(button("Next line")).toBeEnabled();
    expect(button("Set Home Here")).toBeDisabled();

    await user.click(button("Resume"));
    expect(socket.actions()).toEqual([["resumeRun", "", ""]]);
  });

  it("sends play, pause and stop", async () => {
    const { socket, user } = await renderMain();
    await user.click(button("Play"));
    status(socket, 1);
    await user.click(button("Pause"));
    await user.click(button("Stop"));
    expect(socket.actions()).toEqual([
      ["startRun", "", ""],
      ["pauseRun", "", ""],
      ["stopRun", "", ""],
    ]);
  });
});

describe("MainPage jogging", () => {
  it.each([
    ["Up left", "upLeft"],
    ["Up", "up"],
    ["Up right", "upRight"],
    ["Left", "left"],
    ["Right", "right"],
    ["Down left", "downLeft"],
    ["Down", "down"],
    ["Down right", "downRight"],
  ])("jogs %s by the distance", async (name, direction) => {
    const { socket, user } = await renderMain();
    await user.click(button(name));
    expect(socket.actions()).toEqual([["move", direction, 10]]);
  });

  it("jogs by a typed distance and refuses an invalid one", async () => {
    const { socket, user } = await renderMain();
    const distance = screen.getByLabelText("Distance");
    await user.clear(distance);
    await user.type(distance, "2.5");
    await user.click(button("Up"));
    expect(socket.actions()).toEqual([["move", "up", 2.5]]);

    await user.clear(distance);
    await user.type(distance, "-3");
    expect(distance).toHaveAttribute("aria-invalid", "true");
    expect(distance).toHaveAccessibleDescription("Enter a positive number with at most 3 decimal places.");
    await user.click(button("Down"));
    expect(distance).toHaveFocus();
    expect(socket.actions()).toHaveLength(1);
  });

  it("focuses an empty distance instead of jogging", async () => {
    const { socket, user } = await renderMain();
    await user.clear(screen.getByLabelText("Distance"));
    expect(screen.getByLabelText("Distance")).toHaveAttribute("aria-invalid", "false");
    await user.click(button("Left"));
    expect(screen.getByLabelText("Distance")).toHaveFocus();
    expect(socket.actions()).toEqual([]);
  });

  it("follows distance updates from the server", async () => {
    const { socket } = await renderMain();
    socket.serverMessage("requestedSetting", { setting: "distToMove", value: "0.394" });
    expect(screen.getByLabelText("Distance")).toHaveValue("0.394");
  });

  it("switches units through the server, converting the distance", async () => {
    const { socket, user } = await renderMain();
    await user.click(button("Units: mm"));
    expect(socket.emittedEvents("updateSetting")).toEqual([{ data: { setting: "toInches", value: "0.394" } }]);

    socket.serverMessage("requestedSetting", { setting: "units", value: "INCHES" });
    socket.clearEmitted();
    await user.click(button("Units: in"));
    expect(socket.emittedEvents("updateSetting")).toEqual([{ data: { setting: "toMM", value: "254.000" } }]);
  });

  it("converts the stored distance when the typed one is invalid", async () => {
    const { socket, user } = await renderMain();
    await user.type(screen.getByLabelText("Distance"), "x");
    await user.click(button("Units: mm"));
    expect(socket.emittedEvents("updateSetting")).toEqual([{ data: { setting: "toInches", value: "0.394" } }]);
  });

  it("homes, sets home, returns to center and runs macros", async () => {
    const { socket, user } = await renderMain();
    await user.click(button("Home"));
    await user.click(button("Set Home Here"));
    await user.click(button("Macro 1"));
    await user.click(button("Macro 2"));
    expect(socket.actions()).toEqual([
      ["home", "", ""],
      ["defineHome", "", ""],
      ["macro1", "", ""],
      ["macro2", "", ""],
    ]);
  });

  it("labels macros with their configured titles, falling back to their names", async () => {
    const fetch = fakeFetch({
      "/api/main": mainState(),
      "GET /api/state": {
        ...(await (await fakeFetch()("/api/state")).json()),
        macros: [
          { command: "macro1", title: "Spindle on" },
          { command: "macro2", title: " " },
        ],
      },
    });
    renderApp("/", { fetch });
    expect(await screen.findByRole("button", { name: "Spindle on" })).toBeInTheDocument();
    expect(button("Macro 2")).toBeInTheDocument();
  });

  it("keeps both macro buttons, named by default, when the server lists none", async () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    renderApp("/", { fetch: fakeFetch({ "/api/state": () => jsonResponse({}, 500) }) });
    expect(button("Macro 1")).toBeInTheDocument();
    expect(button("Macro 2")).toBeInTheDocument();
    expect(button("Units: …")).toBeDisabled();
  });

  it("opens the Z-Axis modal from beside the jog pad", async () => {
    const { user } = await renderMain();
    await user.click(screen.getByRole("link", { name: "Z-Axis" }));
    expect(screen.getByRole("dialog", { name: "Z-Axis" })).toBeInTheDocument();
  });
});

const renderZ = async (state = mainState()) => {
  const view = renderApp("/zaxis", { fetch: fakeFetch({ "/api/main": state }) });
  await waitFor(() => expect(view.store.getState().status).not.toBeNull());
  await waitFor(() => expect(screen.getByLabelText("Z distance")).toHaveValue("1"));
  view.socket.clearEmitted();
  return view;
};

describe("Z-Axis modal", () => {
  it("raises and lowers by the Z distance", async () => {
    const { socket, user } = await renderZ();
    await user.click(button("Raise"));
    await user.click(button("Lower"));
    await user.click(button("Set Z Zero Here"));
    await user.click(button("Stop Z"));
    expect(socket.actions()).toEqual([
      ["moveZ", "raise", 1],
      ["moveZ", "lower", 1],
      ["defineZ0", "", ""],
      ["stopZ", "", ""],
    ]);
  });

  it("refuses an invalid Z distance", async () => {
    const { socket, user } = await renderZ();
    const distance = screen.getByLabelText("Z distance");
    await user.type(distance, "abc");
    await user.click(button("Raise"));
    expect(distance).toHaveFocus();
    expect(socket.actions()).toEqual([]);
  });

  it("switches Z units with two decimals like the old popup", async () => {
    const { socket, user } = await renderZ();
    const zUnits = within(screen.getByRole("region", { name: "Z axis" })).getByRole("button", { name: "Z units: mm" });
    await user.click(zUnits);
    expect(socket.emittedEvents("updateSetting")).toEqual([{ data: { setting: "toInchesZ", value: "0.04" } }]);

    socket.serverMessage("requestedSetting", { setting: "unitsZ", value: "INCHES" });
    socket.clearEmitted();
    await user.click(
      within(screen.getByRole("region", { name: "Z axis" })).getByRole("button", { name: "Z units: in" }),
    );
    expect(socket.emittedEvents("updateSetting")).toEqual([{ data: { setting: "toMMZ", value: "25.40" } }]);
  });

  it("enables Touch Plate only when the touch plate is configured", async () => {
    await renderZ();
    expect(button("Touch Plate")).toBeDisabled();
  });

  it("probes with the touch plate", async () => {
    const { socket, user } = await renderZ(mainState({ touchPlate: true }));
    await user.click(button("Touch Plate"));
    expect(socket.actions()).toEqual([["touchZ", "", ""]]);
  });
});

describe("MainPage gcode stepping", () => {
  it("steps lines and Z moves", async () => {
    const { socket, user } = await renderMain();
    await user.click(button("Previous Z move"));
    await user.click(button("Previous line"));
    await user.click(button("Next line"));
    await user.click(button("Next Z move"));
    expect(socket.actions()).toEqual([
      ["moveGcodeZ", -1, ""],
      ["moveGcodeIndex", -1, ""],
      ["moveGcodeIndex", 1, ""],
      ["moveGcodeZ", 1, ""],
    ]);
  });

  it("goes to a 1-based line number", async () => {
    const { socket, user } = await renderMain();
    const line = screen.getByLabelText("Line #");
    await user.clear(line);
    await user.type(line, "42{Enter}");
    expect(socket.actions()).toEqual([["moveGcodeGoto", 41, ""]]);
  });

  it("does not cap the line number at the length of the file loaded on mount", async () => {
    const { socket, user } = await renderMain();
    const line = screen.getByLabelText("Line #");
    expect(line).not.toHaveAttribute("max");
    await user.clear(line);
    await user.type(line, "500");
    await user.click(button("Go to line"));
    expect(socket.actions()).toEqual([["moveGcodeGoto", 499, ""]]);
  });

  it("refuses a line number below 1", async () => {
    const { socket, user } = await renderMain();
    const line = screen.getByLabelText("Line #");
    await user.clear(line);
    await user.type(line, "0");
    expect(button("Go to line")).toBeDisabled();
    await user.type(line, "{Enter}");
    expect(socket.actions()).toEqual([]);
  });

  it("follows the server's gcode position", async () => {
    const { socket } = await renderMain();
    socket.serverMessage("gcodePositionMessage", {
      xval: 1,
      yval: 2,
      zval: -0.5,
      gcodeLine: "G1 Z-0.5",
      gcodeLineIndex: 9,
    });
    expect(screen.getByLabelText("Line #")).toHaveValue(10);
    expect(screen.getByText("G1 Z-0.5")).toBeInTheDocument();
    expect(screen.getByText("Line").nextSibling).toHaveTextContent("10");
    expect(screen.getByText("X 1.00 Y 2.00 Z -0.50")).toBeInTheDocument();
  });

  it("shows a dash when no gcode is loaded", async () => {
    await renderMain(mainState({ gcode: { lineIndex: 0, line: "", lineCount: 0 } }));
    expect(screen.getByLabelText("Line #")).not.toHaveAttribute("max");
    expect(screen.getByRole("status", { name: "Current gcode line" })).toHaveTextContent("No line");
  });
});

describe("MainPage alarms", () => {
  it("shows the alarm, pulses Stop and clears it with a stop", async () => {
    const { socket, user } = await renderMain();
    const run = within(screen.getByRole("region", { name: "Run" }));
    expect(run.getByRole("status")).toHaveTextContent("No alarms");

    socket.serverMessage("alarm", { message: "Alarm: Position Lost.  Reset Chains." });
    expect(run.getByRole("status")).toHaveTextContent("Alarm: Position Lost. Reset Chains.");
    expect(run.getByRole("button", { name: "Stop" })).toHaveAttribute("data-alarm", "true");

    await user.click(run.getByRole("button", { name: "Stop and clear alarm" }));
    expect(socket.actions()).toEqual([["stopRun", "", ""]]);
    socket.serverMessage("clearAlarm", { data: "" });
    expect(run.getByRole("status")).toHaveTextContent("No alarms");
  });
});

describe("MainPage readouts and console", () => {
  it("shows sled, home, gcode target, progress, tool and chain error", async () => {
    const { socket } = await renderMain();
    expect(screen.getByText("Sled").nextSibling).toHaveTextContent("—");
    socket.serverMessage("positionMessage", {
      xval: 12.345,
      yval: -6,
      zval: 0.5,
      vel: 300,
      rqvel: 400,
      pcom: "12.5%",
      state: "Idle",
      lpwr: 20,
      rpwr: 30,
      zpwr: 250,
    });
    socket.serverMessage("homePositionMessage", { xval: "1", yval: "2" });
    socket.serverMessage("errorValueMessage", {
      leftError: 0.25,
      rightError: 0.5,
      computedX: 12.3,
      computedY: -6.1,
      computedEnabled: 1,
    });

    expect(screen.getByText("X 12.35 Y -6.00 Z 0.50")).toBeInTheDocument();
    expect(screen.getByText("X 1.00 Y 2.00")).toBeInTheDocument();
    expect(screen.getByText("12.5%")).toBeInTheDocument();
    expect(screen.getByText("State").nextSibling).toHaveTextContent("Idle");
    expect(screen.getByText("Tool").nextSibling).toHaveTextContent("1");
    expect(screen.getByRole("meter", { name: "Left chain error" })).toHaveAttribute("value", "25");
    expect(screen.getByRole("meter", { name: "Right chain error" })).toHaveAttribute("value", "50");
  });

  it("logs controller messages and scrolls to the newest", async () => {
    const { socket } = await renderMain();
    expect(screen.getByText("No messages yet.")).toBeInTheDocument();

    socket.serverMessage("controllerMessage", "Grbl v1.1");
    socket.serverMessage("controllerMessage", "ok");
    const log = screen.getByRole("log", { name: "Controller Messages" });
    expect(
      within(log)
        .getAllByRole("listitem")
        .map((item) => item.textContent),
    ).toEqual(["Grbl v1.1", "ok"]);
  });
});

describe("MainPage job state", () => {
  const job = () => screen.getByText("Job").nextSibling;

  it("reads Idle, Running, Paused and Waiting for tool change", async () => {
    const { socket } = await renderMain();
    expect(job()).toHaveTextContent("Idle");
    socket.serverMessage("statusMessage", { uploadFlag: 1, positioningMode: 0, currentTool: 1, jobInProgress: true });
    expect(job()).toHaveTextContent("Running");
    socket.serverMessage("statusMessage", { uploadFlag: 0, positioningMode: 0, currentTool: 1, jobInProgress: true });
    expect(job()).toHaveTextContent("Paused");
    expect(button("Play")).toBeDisabled();
    socket.serverMessage("statusMessage", {
      uploadFlag: 0,
      positioningMode: 0,
      currentTool: 1,
      jobInProgress: true,
      manualZAxisAdjust: true,
    });
    expect(job()).toHaveTextContent("Waiting for tool change");
  });

  it("shows a dash before the run status is known", () => {
    renderApp("/", { fetch: fakeFetch({ "/api/main": () => new Promise<Response>(() => {}) }) });
    expect(job()).toHaveTextContent("—");
  });
});
