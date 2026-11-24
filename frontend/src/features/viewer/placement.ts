import type { Fetch } from "../../api/http";
import type { LastRun } from "../../protocol";
import { getLastRun } from "./api";
import { isOnBed } from "./shapes";
import type { MachineGeometry, Point } from "./shapes";
import { decodeGcodePayload } from "./viewerState";

export interface Ghost {
  cuts: Float32Array;
  home: Point;
}

/** The toolpath's lowest and highest corners, relative to home; null when it has no points. */
export interface Bounds {
  min: Point;
  max: Point;
}

const grow = (bounds: Bounds | null, x: number, y: number): Bounds =>
  bounds
    ? {
        min: { x: Math.min(bounds.min.x, x), y: Math.min(bounds.min.y, y) },
        max: { x: Math.max(bounds.max.x, x), y: Math.max(bounds.max.y, y) },
      }
    : { min: { x, y }, max: { x, y } };

export const toolpathBounds = (toolpath: ArrayLike<number>[]): Bounds | null =>
  toolpath.reduce<Bounds | null>(
    (bounds, segments) =>
      Array.from({ length: Math.floor(segments.length / 3) }).reduce<Bounds | null>(
        (inner, _, i) => grow(inner, segments[i * 3], segments[i * 3 + 1]),
        bounds,
      ),
    null,
  );

export const fitsOnBed = (machine: MachineGeometry, bounds: Bounds | null, home: Point) =>
  !bounds ||
  [bounds.min, bounds.max].every((corner) => isOnBed(machine, { x: corner.x + home.x, y: corner.y + home.y }));

export const addPoints = (a: Point, b: Point): Point => ({ x: a.x + b.x, y: a.y + b.y });

export const ghostFrom = (lastRun: LastRun): Ghost | null => {
  if (!lastRun) return null;
  const decoded = decodeGcodePayload(lastRun.gcode);
  return "value" in decoded && decoded.value
    ? { cuts: decoded.value.cuts, home: { x: lastRun.homeX, y: lastRun.homeY } }
    : null;
};

/** The last run's toolpath, or null when there is none or it cannot be read. */
export const loadGhost = (fetchImpl: Fetch) =>
  getLastRun(fetchImpl).then(ghostFrom, (error: unknown) => {
    console.warn("Could not load the last run", error);
    return null;
  });
