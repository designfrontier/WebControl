import type { GcodeMove } from "../../protocol";
import { circleSegments, polylineSegments } from "./shapes";
import type { Point3, Segments } from "./shapes";

export interface GcodeLabel {
  text: string;
  position: Point3;
}

/** The whole toolpath as four flat segment buffers, so each draws as one LineSegments object. */
export interface GcodeGeometry {
  cuts: Float32Array;
  rapids: Float32Array;
  plunges: Float32Array;
  events: Float32Array;
  labels: GcodeLabel[];
}

interface Parts {
  cuts: Segments;
  rapids: Segments;
  plunges: Segments;
  events: Segments;
  labels: GcodeLabel[];
}

const SPINDLE_COMMANDS = ["SpindleOnCW", "SpindleOnCCW", "SpindleOff"];
/** GCodeFile sizes its marker circles in 32nds of an inch. */
const MARKER_UNIT = 1 / 32;
const EVENT_RADIUS = 2.25 * MARKER_UNIT;

const offsets = (at: Point3, pairs: [number, number][]): Segments =>
  polylineSegments(pairs.map(([dx, dy]) => [at.x + dx * MARKER_UNIT, at.y + dy * MARKER_UNIT, at.z] as const));

/** Spindle markers: an hourglass for clockwise and off (smaller), a bow tie for counter-clockwise. */
const spindleGlyph = (at: Point3, command: string) => {
  const f = command === "SpindleOff" ? 0.707107 * 2.25 : 3.25;
  return command === "SpindleOnCCW"
    ? offsets(at, [
        [0, 0],
        [f, f],
        [-f, f],
        [f, -f],
        [-f, -f],
        [0, 0],
      ])
    : offsets(at, [
        [0, 0],
        [f, f],
        [f, -f],
        [-f, f],
        [-f, -f],
        [0, 0],
      ]);
};

const markerParts = (move: GcodeMove): Partial<Parts> => {
  const [start, info] = move.points;
  if (!start || !info) return {};
  const at = { x: start[0], y: start[1], z: start[2] };
  const plunge = [start[0], start[1], start[2], start[0], start[1], info[2]];
  const command = move.command ?? "";
  if (SPINDLE_COMMANDS.includes(command)) {
    return {
      cuts: plunge,
      events: [...circleSegments(at, EVENT_RADIUS, 16), ...spindleGlyph(at, command)],
      labels: command === "SpindleOff" ? [] : [{ text: `S${info[1]}`, position: at }],
    };
  }
  if (command === "ToolChange") {
    return {
      cuts: plunge,
      events: circleSegments(at, EVENT_RADIUS, 16),
      labels: [{ text: `T${info[1]}`, position: at }],
    };
  }
  return { cuts: plunge, plunges: circleSegments(at, info[0] * MARKER_UNIT, 16) };
};

const moveParts = (move: GcodeMove): Partial<Parts> =>
  move.type !== "line"
    ? markerParts(move)
    : move.dashed
      ? { rapids: polylineSegments(move.points) }
      : { cuts: polylineSegments(move.points) };

/** Builds the toolpath buffers from GCodeFile.line3D (inches, relative to home). */
export const buildGcodeGeometry = (moves: readonly GcodeMove[]): GcodeGeometry => {
  const parts = moves.map(moveParts);
  const gather = (key: Exclude<keyof Parts, "labels">) => new Float32Array(parts.flatMap((part) => part[key] ?? []));
  return {
    cuts: gather("cuts"),
    rapids: gather("rapids"),
    plunges: gather("plunges"),
    events: gather("events"),
    labels: parts.flatMap((part) => part.labels ?? []),
  };
};

export const segmentCount = (buffer: Float32Array) => buffer.length / 6;
