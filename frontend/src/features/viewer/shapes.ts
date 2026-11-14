import type { BoardInfo, ViewerSnapshot } from "../../protocol";

export interface Point {
  x: number;
  y: number;
}
export interface Point3 extends Point {
  z: number;
}

/** Bed and motor layout in inches, centred on the bed. */
export interface MachineGeometry {
  bedWidth: number;
  bedHeight: number;
  motorOffsetY: number;
  motorSpacingX: number;
}

/** settings.py defaults: an 8x4 ft bed with motors 463 mm above it and 2978.4 mm apart. */
export const DEFAULT_MACHINE: MachineGeometry = {
  bedWidth: 96,
  bedHeight: 48,
  motorOffsetY: 18.2283,
  motorSpacingX: 117.2598,
};

export const machineFromSnapshot = ({ machine }: ViewerSnapshot): MachineGeometry => ({
  bedWidth: machine.bedWidth && machine.bedWidth > 0 ? machine.bedWidth : DEFAULT_MACHINE.bedWidth,
  bedHeight: machine.bedHeight && machine.bedHeight > 0 ? machine.bedHeight : DEFAULT_MACHINE.bedHeight,
  motorOffsetY: machine.motorOffsetY ?? DEFAULT_MACHINE.motorOffsetY,
  motorSpacingX: machine.motorSpacingX ?? DEFAULT_MACHINE.motorSpacingX,
});

/** Flat xyz triples; consecutive pairs form one line segment. */
export type Segments = number[];

export const polylineSegments = (points: readonly (readonly [number, number, number])[]): Segments =>
  points.slice(1).flatMap((point, i) => [...points[i], ...point]);

export const circleSegments = ({ x, y, z }: Point3, radius: number, sides = 32): Segments =>
  Array.from({ length: sides }, (_, i) => {
    const a0 = (i / sides) * Math.PI * 2;
    const a1 = ((i + 1) / sides) * Math.PI * 2;
    return [
      x + radius * Math.cos(a0),
      y + radius * Math.sin(a0),
      z,
      x + radius * Math.cos(a1),
      y + radius * Math.sin(a1),
      z,
    ];
  }).flat();

/** The old UI's position markers: a cross of half-length `arm` over a circle, centred on the origin. */
export const crosshair = (arm: number, radius: number): Segments => [
  -arm,
  0,
  0,
  arm,
  0,
  0,
  0,
  -arm,
  0,
  0,
  arm,
  0,
  ...circleSegments({ x: 0, y: 0, z: 0 }, radius),
];

const GRID_SPACING = 24;

const gridStops = (extent: number) => {
  const count = Math.floor(extent / 2 / GRID_SPACING);
  return Array.from({ length: count * 2 + 1 }, (_, i) => (i - count) * GRID_SPACING).filter(
    (stop) => Math.abs(stop) < extent / 2,
  );
};

/** The bed outline and a dashed 2 ft grid, like the old fixed 8x4 ft frame but sized to the machine. */
export const bedFrame = ({ bedWidth, bedHeight }: MachineGeometry) => {
  const [w, h] = [bedWidth / 2, bedHeight / 2];
  const corners: [number, number, number][] = [
    [-w, h, 0],
    [w, h, 0],
    [w, -h, 0],
    [-w, -h, 0],
    [-w, h, 0],
  ];
  return {
    outline: polylineSegments(corners),
    grid: [
      ...gridStops(bedWidth).flatMap((x) => [x, h, 0, x, -h, 0]),
      ...gridStops(bedHeight).flatMap((y) => [-w, y, 0, w, y, 0]),
    ],
  };
};

export const motorPositions = ({ bedHeight, motorOffsetY, motorSpacingX }: MachineGeometry): [Point, Point] => {
  const y = bedHeight / 2 + motorOffsetY;
  return [
    { x: -motorSpacingX / 2, y },
    { x: motorSpacingX / 2, y },
  ];
};

export const chainSegments = (machine: MachineGeometry, sled: Point3): Segments =>
  motorPositions(machine).flatMap((motor) => [motor.x, motor.y, sled.z, sled.x, sled.y, sled.z]);

const clamp = (value: number, low: number, high: number) => Math.min(high, Math.max(low, value));

/** Light guide lines through the cursor, clamped to the bed, just behind the bed plane. */
export const guideSegments = ({ bedWidth, bedHeight }: MachineGeometry, cursor: Point): Segments => {
  const [w, h] = [bedWidth / 2, bedHeight / 2];
  const x = clamp(cursor.x, -w, w);
  const y = clamp(cursor.y, -h, h);
  return [x, h, -0.001, x, -h, -0.001, w, y, -0.001, -w, y, -0.001];
};

export const isOnBed = ({ bedWidth, bedHeight }: MachineGeometry, { x, y }: Point) =>
  Math.abs(x) <= bedWidth / 2 && Math.abs(y) <= bedHeight / 2;

/** Two triangles per cut square inch of the board, matching boardManager's grid (index = x + y * columns). */
export const cutCellTriangles = (board: BoardInfo, cells: readonly unknown[]): number[] => {
  const columns = Math.ceil(board.width);
  const rows = Math.ceil(board.height);
  const z = 0.01;
  return cells.slice(0, columns * rows).flatMap((cut, index) => {
    if (!cut) return [];
    const cx = (index % columns) - columns / 2 + board.centerX;
    const cy = Math.floor(index / columns) - rows / 2 + board.centerY;
    const [l, r, b, t] = [cx - 0.5, cx + 0.5, cy - 0.5, cy + 0.5];
    return [l, b, z, r, b, z, r, t, z, l, b, z, r, t, z, l, t, z];
  });
};

const VIEW_MARGIN = 1.08;

/** Half extents of an orthographic view that shows the whole bed at this aspect ratio. */
export const fitOrthographic = ({ bedWidth, bedHeight }: MachineGeometry, aspect: number) => {
  const halfHeight = Math.max(bedHeight / 2, bedWidth / 2 / aspect) * VIEW_MARGIN;
  return { halfWidth: halfHeight * aspect, halfHeight };
};

/** Camera distance at which a perspective camera with this vertical FOV shows the whole bed. */
export const fitPerspectiveDistance = (machine: MachineGeometry, aspect: number, fovDegrees: number) =>
  fitOrthographic(machine, aspect).halfHeight / Math.tan(((fovDegrees / 2) * Math.PI) / 180);
