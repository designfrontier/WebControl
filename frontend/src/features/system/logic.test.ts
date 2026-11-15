import { describe, expect, it } from "vitest";
import { isAllowed } from "./actionsCatalog";
import type { AllowedDuring } from "./actionsCatalog";
import { bounds, pathFor } from "./chartMath";
import { helpRoute } from "./helpLinks";
import { parsePidMessage, toChart } from "./pidData";
import type { RunState } from "../../state/machine";

describe("isAllowed mirrors processAction's run-state gates", () => {
  it.each<[AllowedDuring, RunState, boolean]>([
    ["always", "running", true],
    ["paused", "running", false],
    ["paused", "paused", true],
    ["paused", "idle", true],
    ["idle", "paused", false],
    ["idle", "running", false],
    ["idle", "idle", true],
    ["idle", "unknown", true],
  ])("%s during %s is %s", (allowed, runState, expected) => {
    expect(isAllowed(allowed, runState)).toBe(expected);
  });

  it.each<[RunState, boolean, boolean]>([
    ["idle", false, true],
    ["idle", true, false],
    ["paused", false, false],
  ])("noJob during %s with gcode locked %s is %s", (runState, gcodeLocked, expected) => {
    expect(isAllowed("noJob", runState, gcodeLocked)).toBe(expected);
  });
});

describe("toChart", () => {
  it("plots version 1 results as a single error series", () => {
    expect(toChart({ result: "velocity", version: "1", data: [1.5, "2", -3] })).toEqual({
      kind: "velocity",
      series: [{ name: "Error", values: [1.5, 2, -3] }],
    });
  });

  it("splits version 2 velocity rows into setpoint, input and output", () => {
    const chart = toChart({ result: "velocity", version: "2", data: ["1,2,3", "4,5,6"] });
    expect(chart.series).toEqual([
      { name: "Setpoint", values: [1, 4] },
      { name: "Input", values: [2, 5] },
      { name: "Output", values: [3, 6] },
    ]);
  });

  it("splits version 2 position rows into five series, padding short rows with gaps", () => {
    const chart = toChart({ result: "position", version: "2", data: ["1,2,3,4,5", "6,7"] });
    expect(chart.series.map((s) => s.name)).toEqual(["Setpoint", "Input", "Output", "RPM input", "Voltage"]);
    expect(chart.series[4].values).toEqual([5, NaN]);
  });
});

describe("parsePidMessage", () => {
  const result = { result: "position", version: 1, data: [0.5] };

  it("reads the result object UIProcessor wraps in { command, data }", () => {
    expect(parsePidMessage({ data: result })).toEqual({ kind: "position", series: [{ name: "Error", values: [0.5] }] });
  });

  it("also reads the result as JSON text", () => {
    expect(parsePidMessage({ data: JSON.stringify(result) })?.kind).toBe("position");
  });

  it.each([["{broken"], [{ result: "torque", version: "1", data: [] }]])("ignores unusable payloads", (data) => {
    expect(parsePidMessage({ data })).toBeNull();
  });
});

describe("chart math", () => {
  it("spans every finite value across series", () => {
    expect(
      bounds([
        { name: "a", values: [1, NaN, 5] },
        { name: "b", values: [-2] },
      ]),
    ).toEqual({
      min: -2,
      max: 5,
      length: 3,
    });
  });

  it("pads a flat line and defaults an empty chart", () => {
    expect(bounds([{ name: "a", values: [3, 3] }])).toEqual({ min: 2, max: 4, length: 2 });
    expect(bounds([])).toEqual({ min: 0, max: 1, length: 0 });
  });

  it("draws left to right, top is max, and breaks the line at gaps", () => {
    const path = pathFor([1, 0, NaN, 1], { min: 0, max: 1, length: 4 });
    expect(path).toBe("M56.0,12.0 L233.3,272.0 M588.0,12.0");
  });

  it("places a single sample at the left edge", () => {
    expect(pathFor([0], { min: 0, max: 1, length: 1 })).toBe("M56.0,272.0");
  });
});

describe("helpRoute", () => {
  it.each([
    ["help", "/help"],
    ["GettingStarted/firstSteps.md", "/help/GettingStarted/firstSteps.md"],
    ["../Actions/index.md#top", "/help/Actions/index.md#top"],
    ["/Boards/index.md", "/help/Boards/index.md"],
    ["notes.txt", null],
  ])("%s -> %s", (href, route) => {
    expect(helpRoute(href)).toBe(route);
  });
});
