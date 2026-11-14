import { describe, expect, it } from "vitest";
import {
  COLUMNS,
  ROWS,
  SPACING_MM,
  bounds,
  calSurface,
  curveSeries,
  errorSeries,
  project,
  xyErrorPoints,
} from "./chartData";

describe("calSurface", () => {
  it("evaluates the quadratic surface at 3-inch grid offsets", () => {
    const x = 2 * SPACING_MM;
    const y = -1 * SPACING_MM;
    expect(calSurface([1, 2, 3, 4, 5, 6], 2, -1)).toBeCloseTo(5 * x ** 2 + 6 * y ** 2 + 4 * x * y + 2 * x + 3 * y + 1);
  });

  it("treats missing coefficients as zero", () => {
    expect(calSurface([7], 3, 3)).toBe(7);
  });
});

describe("curveSeries", () => {
  it("has one line per row and one point per column, centred on the middle square", () => {
    const series = curveSeries([0, 1, 0, 0, 0, 0]);
    expect(series).toHaveLength(ROWS);
    expect(series.every((line) => line.length === COLUMNS)).toBe(true);
    expect(series[7][15]).toBe(0);
    expect(series[0][0]).toBeCloseTo(-15 * SPACING_MM);
  });
});

describe("errorSeries", () => {
  it("transposes the column-major grid and fills gaps with zero", () => {
    const grid = Array.from({ length: COLUMNS }, (_, column) =>
      Array.from({ length: ROWS }, (_, row) => column * 100 + row),
    );
    const series = errorSeries(grid);
    expect(series[2][5]).toBe(502);
    expect(errorSeries([])[3][4]).toBe(0);
  });
});

describe("xyErrorPoints", () => {
  it("offsets each ideal grid point by its X and Y error", () => {
    const errorX = [[0.5]];
    const errorY = [[-0.25]];
    const { ideal, measured } = xyErrorPoints(errorX, errorY);
    expect(ideal).toHaveLength(ROWS * COLUMNS);
    expect(ideal[1]).toEqual({ x: SPACING_MM, y: 0 });
    expect(measured[0]).toEqual({ x: 0.5, y: -0.25 });
    expect(measured[1]).toEqual({ x: SPACING_MM, y: 0 });
  });
});

describe("bounds and project", () => {
  it("spans the values, widening a flat or empty set", () => {
    expect(bounds([3, -2, 8])).toEqual({ min: -2, max: 8 });
    expect(bounds([4, 4])).toEqual({ min: 3, max: 5 });
    expect(bounds([])).toEqual({ min: -1, max: 1 });
  });

  it("maps into the plot, inverted for the y axis", () => {
    const domain = { min: 0, max: 10 };
    expect(project(2.5, domain, 100)).toBe(25);
    expect(project(2.5, domain, 100, true)).toBe(75);
  });
});
