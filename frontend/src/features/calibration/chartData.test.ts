import { describe, expect, it } from "vitest";
import { COLUMNS, ROWS, cellName, gridCells, heatLevel, sameCell, summarise } from "./chartData";
import { errorGrid } from "./opticalFixtures";

describe("gridCells", () => {
  it("reads the [i+15][7-j] grids top row first, left to right", () => {
    const cells = gridCells(
      errorGrid((i, j) => i * 10 + j),
      errorGrid((i, j) => -(i * 10 + j)),
    );
    expect(cells).toHaveLength(COLUMNS * ROWS);
    expect(cells[0]).toEqual({ i: -15, j: 7, error: { x: -143, y: 143 } });
    expect(cells[COLUMNS * ROWS - 1]).toEqual({ i: 15, j: -7, error: { x: 143, y: -143 } });
    expect(cells[COLUMNS]).toMatchObject({ i: -15, j: 6 });
  });

  it("leaves a square unmeasured when either axis is missing", () => {
    const cells = gridCells(
      errorGrid(() => 1),
      errorGrid((i) => (i === 0 ? null : 1)),
    );
    expect(cells.find(({ i, j }) => i === 0 && j === 0)?.error).toBeNull();
    expect(gridCells([], [])[0].error).toBeNull();
  });
});

describe("summarise", () => {
  it("counts the measured squares, the largest error and the RMS", () => {
    const cells = [
      { i: 0, j: 0, error: { x: 3, y: 4 } },
      { i: 1, j: 0, error: { x: 0, y: 0 } },
      { i: 2, j: 0, error: null },
    ];
    expect(summarise(cells)).toEqual({ measured: 2, largest: 5, rms: Math.sqrt(12.5) });
  });

  it("is all zeros with nothing measured", () => {
    expect(summarise([{ i: 0, j: 0, error: null }])).toEqual({ measured: 0, largest: 0, rms: 0 });
  });
});

describe("heatLevel", () => {
  it("buckets an error by its share of the largest", () => {
    expect([0, 0.19, 0.2, 0.59, 0.99, 1].map((size) => heatLevel(size, 1))).toEqual([0, 0, 1, 2, 4, 4]);
    expect(heatLevel(0, 0)).toBe(0);
  });
});

describe("cells", () => {
  it("compares and names squares", () => {
    expect(sameCell({ i: 1, j: 2 }, { i: 1, j: 2 })).toBe(true);
    expect(sameCell({ i: 1, j: 3 }, { i: 1, j: 2 })).toBe(false);
    expect(sameCell(null, { i: 1, j: 2 })).toBe(false);
    expect(cellName({ i: -3, j: 7 })).toBe("column -3, row 7");
  });
});
