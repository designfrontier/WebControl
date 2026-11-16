import type { OpticalCell } from "../../protocol";

/** The printed target: 31 columns (i = -15..15) by 15 rows (j = -7..7) of squares. */
export const COLUMNS = 31;
export const ROWS = 15;
export const HEAT_LEVELS = 5;

type Grid = readonly (readonly (number | null)[])[];

export interface GridCell extends OpticalCell {
  error: { x: number; y: number } | null;
}

const range = (length: number) => Array.from({ length }, (_, index) => index);

/** Every square, top row first, from the server's grids indexed [i+15][7-j]. */
export const gridCells = (errorX: Grid, errorY: Grid): GridCell[] =>
  range(ROWS).flatMap((row) =>
    range(COLUMNS).map((column) => {
      const x = errorX[column]?.[row] ?? null;
      const y = errorY[column]?.[row] ?? null;
      return { i: column - 15, j: 7 - row, error: x === null || y === null ? null : { x, y } };
    }),
  );

export const magnitude = ({ x, y }: { x: number; y: number }) => Math.hypot(x, y);

export const summarise = (cells: readonly GridCell[]) => {
  const sizes = cells.flatMap(({ error }) => (error ? [magnitude(error)] : []));
  const squares = sizes.reduce((sum, size) => sum + size * size, 0);
  return {
    measured: sizes.length,
    largest: Math.max(0, ...sizes),
    rms: sizes.length ? Math.sqrt(squares / sizes.length) : 0,
  };
};

/** 0 to HEAT_LEVELS - 1: how big `size` is next to the largest error measured. */
export const heatLevel = (size: number, largest: number) =>
  largest > 0 ? Math.min(HEAT_LEVELS - 1, Math.floor((size / largest) * HEAT_LEVELS)) : 0;

export const sameCell = (a: OpticalCell | null | undefined, b: OpticalCell) => a?.i === b.i && a.j === b.j;

export const cellName = ({ i, j }: OpticalCell) => `column ${i}, row ${j}`;
