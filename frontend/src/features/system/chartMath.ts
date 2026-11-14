import type { Series } from "./pidData";

export const WIDTH = 600;
export const HEIGHT = 300;
export const PAD = { left: 56, right: 12, top: 12, bottom: 28 };

export interface Bounds {
  min: number;
  max: number;
  length: number;
}

export const bounds = (series: Series[]): Bounds => {
  const finite = series.flatMap((s) => s.values).filter(Number.isFinite);
  const length = series.reduce((longest, s) => Math.max(longest, s.values.length), 0);
  if (finite.length === 0) return { min: 0, max: 1, length };
  const min = finite.reduce((a, b) => Math.min(a, b));
  const max = finite.reduce((a, b) => Math.max(a, b));
  return min === max ? { min: min - 1, max: max + 1, length } : { min, max, length };
};

/** SVG path data for one series; gaps (NaN) start a new subpath instead of dropping to zero. */
export const pathFor = (values: number[], { min, max, length }: Bounds) => {
  const x = (i: number) => PAD.left + (length > 1 ? (i / (length - 1)) * (WIDTH - PAD.left - PAD.right) : 0);
  const y = (v: number) => PAD.top + (1 - (v - min) / (max - min)) * (HEIGHT - PAD.top - PAD.bottom);
  return values
    .map((value, i) => ({ value, i }))
    .filter(({ value }) => Number.isFinite(value))
    .map(({ value, i }, n, points) => {
      const move = n === 0 || points[n - 1].i !== i - 1;
      return `${move ? "M" : "L"}${x(i).toFixed(1)},${y(value).toFixed(1)}`;
    })
    .join(" ");
};
