import { describe, expect, it } from "vitest";
import {
  DEFAULT_MACHINE,
  bedFrame,
  bedViewFrame,
  chainSegments,
  circleSegments,
  crosshair,
  cutCellTriangles,
  fitOrthographic,
  fitPerspectiveDistance,
  guideSegments,
  isOnBed,
  machineFromSnapshot,
  motorPositions,
  polylineSegments,
  viewFrame,
} from "./shapes";
import type { Segments } from "./shapes";

/** Splits flat xyz pairs into [[x0, y0, z0], [x1, y1, z1]] segments for readable assertions. */
const pairs = (segments: Segments) =>
  Array.from({ length: segments.length / 6 }, (_, i) => [
    segments.slice(i * 6, i * 6 + 3),
    segments.slice(i * 6 + 3, i * 6 + 6),
  ]);

const board = {
  width: 3,
  height: 2,
  thickness: 0.75,
  centerX: 10,
  centerY: -5,
  boardID: "b",
  material: "m",
  fileName: "",
};

describe("machineFromSnapshot", () => {
  it("uses the server's geometry and falls back to defaults for missing or unusable values", () => {
    const snapshot = (machine: Partial<Record<keyof typeof DEFAULT_MACHINE, number | null>>) => ({
      machine: { bedWidth: null, bedHeight: null, motorOffsetY: null, motorSpacingX: null, ...machine },
      gcode: null,
      board: null,
      cutData: null,
    });
    expect(
      machineFromSnapshot(snapshot({ bedWidth: 120, bedHeight: 60, motorOffsetY: 20, motorSpacingX: 140 })),
    ).toEqual({ bedWidth: 120, bedHeight: 60, motorOffsetY: 20, motorSpacingX: 140 });
    expect(machineFromSnapshot(snapshot({ bedWidth: 0, bedHeight: -1 }))).toEqual(DEFAULT_MACHINE);
  });
});

describe("segments", () => {
  it("turns a polyline into consecutive segments", () => {
    expect(
      pairs(
        polylineSegments([
          [0, 0, 0],
          [1, 0, 0],
          [1, 1, -1],
        ]),
      ),
    ).toEqual([
      [
        [0, 0, 0],
        [1, 0, 0],
      ],
      [
        [1, 0, 0],
        [1, 1, -1],
      ],
    ]);
    expect(polylineSegments([[0, 0, 0]])).toEqual([]);
  });

  it("closes a circle of the requested radius", () => {
    const circle = circleSegments({ x: 1, y: 2, z: 3 }, 2, 4);
    expect(circle).toHaveLength(4 * 6);
    expect(circle.slice(0, 3)).toEqual([3, 2, 3]);
    expect(circle.at(-3)).toBeCloseTo(3);
    pairs(circle).forEach(([[x, y, z]]) => {
      expect(Math.hypot(x - 1, y - 2)).toBeCloseTo(2);
      expect(z).toBe(3);
    });
  });

  it("draws a crosshair as two arms and a circle", () => {
    const marker = crosshair(1.5, 1);
    expect(marker.slice(0, 12)).toEqual([-1.5, 0, 0, 1.5, 0, 0, 0, -1.5, 0, 0, 1.5, 0]);
    expect(marker).toHaveLength(12 + 32 * 6);
  });
});

describe("bedFrame", () => {
  it("matches the old fixed frame for the default 8x4 ft bed", () => {
    const { outline, grid } = bedFrame(DEFAULT_MACHINE);
    expect(pairs(outline).map(([start]) => start)).toEqual([
      [-48, 24, 0],
      [48, 24, 0],
      [48, -24, 0],
      [-48, -24, 0],
    ]);
    expect(pairs(grid)).toEqual([
      [
        [-24, 24, 0],
        [-24, -24, 0],
      ],
      [
        [0, 24, 0],
        [0, -24, 0],
      ],
      [
        [24, 24, 0],
        [24, -24, 0],
      ],
      [
        [-48, 0, 0],
        [48, 0, 0],
      ],
    ]);
  });

  it("keeps grid lines inside smaller beds", () => {
    expect(pairs(bedFrame({ ...DEFAULT_MACHINE, bedWidth: 20, bedHeight: 10 }).grid)).toHaveLength(2);
  });
});

describe("chains and guides", () => {
  it("runs a chain from each motor to the sled", () => {
    const [left, right] = motorPositions(DEFAULT_MACHINE);
    expect(left).toEqual({ x: -58.6299, y: 24 + 18.2283 });
    expect(right.x).toBe(58.6299);
    expect(pairs(chainSegments(DEFAULT_MACHINE, { x: 1, y: 2, z: 0.5 }))).toEqual([
      [
        [left.x, left.y, 0.5],
        [1, 2, 0.5],
      ],
      [
        [right.x, right.y, 0.5],
        [1, 2, 0.5],
      ],
    ]);
  });

  it("clamps the cursor guides to the bed", () => {
    expect(guideSegments(DEFAULT_MACHINE, { x: 100, y: -3 })).toEqual([
      48, 24, -0.001, 48, -24, -0.001, 48, -3, -0.001, -48, -3, -0.001,
    ]);
  });

  it("knows whether a point is on the bed", () => {
    expect(isOnBed(DEFAULT_MACHINE, { x: 48, y: -24 })).toBe(true);
    expect(isOnBed(DEFAULT_MACHINE, { x: 48.1, y: 0 })).toBe(false);
    expect(isOnBed(DEFAULT_MACHINE, { x: 0, y: 30 })).toBe(false);
  });
});

describe("cutCellTriangles", () => {
  it("draws one unit square per cut cell, centred on the board like boardManager's grid", () => {
    const triangles = cutCellTriangles(board, [false, true, false, false, false, true, true]);
    expect(triangles).toHaveLength(2 * 18);
    const xs = (from: number) => [0, 3, 6, 9, 12, 15].map((i) => triangles[from + i]);
    const ys = (from: number) => [1, 4, 7, 10, 13, 16].map((i) => triangles[from + i]);
    // Cell 1 is column 1 of row 0; cell 5 is column 2 of row 1; cell 6 is past the 3x2 grid.
    expect([Math.min(...xs(0)), Math.max(...xs(0)), Math.min(...ys(0)), Math.max(...ys(0))]).toEqual([
      9, 10, -6.5, -5.5,
    ]);
    expect([Math.min(...xs(18)), Math.max(...xs(18)), Math.min(...ys(18)), Math.max(...ys(18))]).toEqual([
      10, 11, -5.5, -4.5,
    ]);
    expect(triangles[2]).toBe(0.01);
  });

  it("rounds fractional boards up to whole cells", () => {
    expect(cutCellTriangles({ ...board, width: 2.5, height: 1.2 }, Array(6).fill(1))).toHaveLength(6 * 18);
  });
});

describe("camera fitting", () => {
  it("fits the bed width on tall screens and the height on wide ones", () => {
    const bed = bedViewFrame(DEFAULT_MACHINE);
    const narrow = fitOrthographic(bed, 1);
    expect(narrow.halfWidth).toBeCloseTo(48 * 1.08);
    expect(narrow.halfHeight).toBeCloseTo(48 * 1.08);
    const wide = fitOrthographic(bed, 4);
    expect(wide.halfHeight).toBeCloseTo(24 * 1.08);
    expect(wide.halfWidth).toBeCloseTo(96 * 1.08);
  });

  it("backs the perspective camera off far enough to show the same area", () => {
    expect(fitPerspectiveDistance(bedViewFrame(DEFAULT_MACHINE), 2, 90)).toBeCloseTo(24 * 1.08);
  });

  it("frames just the bed when the toolpath stays on it", () => {
    const inside = Float32Array.from([0, 0, 0, 10, 10, 0]);
    expect(viewFrame(DEFAULT_MACHINE, [inside], { x: 0, y: 0 })).toEqual({ x: 0, y: 0, width: 96, height: 48 });
  });

  it("grows the frame to take in toolpath that runs off the bed, relative to home", () => {
    const offBed = [40, 0, 0, 60, 30, 0];
    expect(viewFrame(DEFAULT_MACHINE, [offBed], { x: 10, y: 0 })).toEqual({ x: 11, y: 3, width: 118, height: 54 });
  });
});
