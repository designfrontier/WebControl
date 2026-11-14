import { describe, expect, it } from "vitest";
import type { GcodeMove } from "../../protocol";
import { buildGcodeGeometry, segmentCount } from "./gcodeGeometry";

const line = (points: GcodeMove["points"], dashed = false): GcodeMove => ({
  type: "line",
  points,
  dashed,
  command: null,
});
const marker = (command: string | null, at: [number, number, number], info: [number, number, number]): GcodeMove => ({
  type: "circle",
  points: [at, info],
  dashed: false,
  command,
});

describe("buildGcodeGeometry", () => {
  it("puts feed moves in cuts and G0 rapids in the dashed buffer", () => {
    const geometry = buildGcodeGeometry([
      line([
        [0, 0, 0],
        [1, 0, -0.1],
        [1, 1, -0.1],
      ]),
      line(
        [
          [1, 1, -0.1],
          [5, 5, 0.2],
        ],
        true,
      ),
    ]);
    expect(Array.from(geometry.cuts)).toEqual([0, 0, 0, 1, 0, -0.1, 1, 0, -0.1, 1, 1, -0.1].map(Math.fround));
    expect(Array.from(geometry.rapids)).toEqual([1, 1, -0.1, 5, 5, 0.2].map(Math.fround));
    expect(segmentCount(geometry.cuts)).toBe(2);
    expect(segmentCount(geometry.rapids)).toBe(1);
    expect(geometry.labels).toEqual([]);
  });

  it("marks a plunge with a green circle sized in 32nds and a vertical line to the target depth", () => {
    const geometry = buildGcodeGeometry([marker(null, [2, 3, 0], [4, 0, -0.25])]);
    expect(Array.from(geometry.cuts)).toEqual([2, 3, 0, 2, 3, -0.25]);
    expect(segmentCount(geometry.plunges)).toBe(16);
    expect(geometry.plunges[0]).toBeCloseTo(2 + 4 / 32);
    expect(geometry.events).toHaveLength(0);
  });

  it.each([
    ["SpindleOnCW", "S12000"],
    ["SpindleOnCCW", "S12000"],
  ])("draws %s in red with a speed label", (command, label) => {
    const geometry = buildGcodeGeometry([marker(command, [1, 1, 0], [3, 12000, 0])]);
    expect(segmentCount(geometry.events)).toBe(16 + 5);
    expect(geometry.labels).toEqual([{ text: label, position: { x: 1, y: 1, z: 0 } }]);
    expect(geometry.plunges).toHaveLength(0);
  });

  it("draws the two spindle glyphs differently", () => {
    const cw = buildGcodeGeometry([marker("SpindleOnCW", [0, 0, 0], [3, 1, 0])]).events;
    const ccw = buildGcodeGeometry([marker("SpindleOnCCW", [0, 0, 0], [4, 1, 0])]).events;
    expect(Array.from(cw)).not.toEqual(Array.from(ccw));
  });

  it("draws spindle off smaller and unlabelled", () => {
    const geometry = buildGcodeGeometry([marker("SpindleOff", [0, 0, 0], [5, 0, 0])]);
    expect(geometry.labels).toEqual([]);
    const glyph = Array.from(geometry.events).slice(16 * 6);
    expect(glyph[3]).toBeCloseTo((0.707107 * 2.25) / 32);
  });

  it("labels tool changes", () => {
    const geometry = buildGcodeGeometry([marker("ToolChange", [1, 2, 3], [6, 2, 3])]);
    expect(geometry.labels).toEqual([{ text: "T2", position: { x: 1, y: 2, z: 3 } }]);
    expect(segmentCount(geometry.events)).toBe(16);
  });

  it("skips markers without their info point", () => {
    const geometry = buildGcodeGeometry([{ type: "circle", points: [[0, 0, 0]], dashed: false, command: null }]);
    expect([geometry.cuts, geometry.plunges, geometry.events].map((buffer) => buffer.length)).toEqual([0, 0, 0]);
  });
});
