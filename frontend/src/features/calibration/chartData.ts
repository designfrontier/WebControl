/** The optical calibration grid: 31 columns by 15 rows of registration squares, 3 inches apart. */
export const COLUMNS = 31;
export const ROWS = 15;
export const SPACING_MM = 3 * 25.4;

export interface ChartPoint {
  x: number;
  y: number;
}

export interface Bounds {
  min: number;
  max: number;
}

const range = (length: number) => Array.from({ length }, (_, index) => index);

/** The fitted quadratic surface at grid offset (column, row) from the centre, as opticalCalibration.js computed it. */
export const calSurface = (coefficients: readonly number[], column: number, row: number) => {
  const [c0 = 0, c1 = 0, c2 = 0, c3 = 0, c4 = 0, c5 = 0] = coefficients;
  const x = column * SPACING_MM;
  const y = row * SPACING_MM;
  return c4 * x ** 2 + c5 * y ** 2 + c3 * x * y + c1 * x + c2 * y + c0;
};

/** One line per grid row, each with a value per column. */
export const curveSeries = (coefficients: readonly number[]) =>
  range(ROWS).map((row) => range(COLUMNS).map((column) => calSurface(coefficients, column - 15, row - 7)));

/** The server sends errors column-major (grid[column][row]); charts want one line per row. */
export const errorSeries = (grid: readonly (readonly number[])[]) =>
  range(ROWS).map((row) => range(COLUMNS).map((column) => grid[column]?.[row] ?? 0));

export const xyErrorPoints = (errorX: readonly (readonly number[])[], errorY: readonly (readonly number[])[]) => {
  const cells = range(ROWS).flatMap((row) => range(COLUMNS).map((column) => ({ row, column })));
  return {
    ideal: cells.map(({ row, column }) => ({ x: column * SPACING_MM, y: row * SPACING_MM })),
    measured: cells.map(({ row, column }) => ({
      x: column * SPACING_MM + (errorX[column]?.[row] ?? 0),
      y: row * SPACING_MM + (errorY[column]?.[row] ?? 0),
    })),
  };
};

/** Min and max of `values`, widened when they are equal so a flat line still has a scale. */
export const bounds = (values: readonly number[]): Bounds => {
  const min = Math.min(...values);
  const max = Math.max(...values);
  if (!Number.isFinite(min) || !Number.isFinite(max)) return { min: -1, max: 1 };
  return min === max ? { min: min - 1, max: max + 1 } : { min, max };
};

/** Maps a value in `domain` to [0, size], flipping when `invert` so larger values sit higher on screen. */
export const project = (value: number, domain: Bounds, size: number, invert = false) => {
  const ratio = (value - domain.min) / (domain.max - domain.min);
  return (invert ? 1 - ratio : ratio) * size;
};
