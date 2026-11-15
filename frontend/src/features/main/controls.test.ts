import { describe, expect, it } from "vitest";
import {
  controlsFor,
  convertDistance,
  fixed,
  parseDistance,
  positioningModeLabel,
  unitsLabel,
  unwrapMessage,
} from "./controls";

describe("controlsFor", () => {
  it.each([
    ["idle", { manual: true, units: true, defineHome: true, play: true, pause: false, step: true }],
    ["running", { manual: false, units: false, defineHome: false, play: false, pause: true, step: false }],
    ["paused", { manual: true, units: true, defineHome: false, play: false, pause: true, step: true }],
    ["unknown", { manual: false, units: false, defineHome: false, play: false, pause: false, step: false }],
  ] as const)("gates the controls while %s", (runState, expected) => {
    expect(controlsFor(runState)).toEqual(expected);
  });

  it("holds home and the unit switches while the gcode is locked", () => {
    expect(controlsFor("paused", true)).toMatchObject({ manual: true, units: false, defineHome: false });
    expect(controlsFor("idle", true)).toMatchObject({ manual: true, units: false, defineHome: false });
  });
});

describe("parseDistance", () => {
  it.each([
    ["10", 10],
    ["0.125", 0.125],
    [" 2.5 ", 2.5],
    ["5.", 5],
    [".5", 0.5],
    ["0", 0],
  ])("accepts %j", (text, value) => {
    expect(parseDistance(text, 3)).toBe(value);
  });

  it.each(["", ".", "-1", "1.2345", "abc", "1e3", "1,5"])("rejects %j", (text) => {
    expect(parseDistance(text, 3)).toBeNull();
  });

  it("allows as many decimals as asked", () => {
    expect(parseDistance("1.23456", 5)).toBe(1.23456);
    expect(parseDistance("1.234567", 5)).toBeNull();
  });
});

describe("unit helpers", () => {
  it("converts like the old unitSwitch", () => {
    expect(convertDistance(25.4, "MM", 3)).toBe("1.000");
    expect(convertDistance(1, "INCHES", 3)).toBe("25.400");
    expect(convertDistance(10, "MM", 2)).toBe("0.39");
  });

  it("names units", () => {
    expect(unitsLabel("MM")).toBe("mm");
    expect(unitsLabel("INCHES")).toBe("in");
  });

  it("formats readouts", () => {
    expect(fixed(1)).toBe("1.00");
    expect(positioningModeLabel(0)).toBe("Absolute (G90)");
    expect(positioningModeLabel(1)).toBe("Incremental (G91)");
  });
});

describe("unwrapMessage", () => {
  it("unwraps json.dumps'd strings", () => {
    expect(unwrapMessage('"No GCode file loaded."')).toBe("No GCode file loaded.");
  });

  it("leaves plain text and non-string JSON alone", () => {
    expect(unwrapMessage("Tool change: T2")).toBe("Tool change: T2");
    expect(unwrapMessage("42")).toBe("42");
  });
});
