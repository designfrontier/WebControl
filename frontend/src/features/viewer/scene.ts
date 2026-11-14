import {
  BoxGeometry,
  BufferAttribute,
  BufferGeometry,
  Color,
  EdgesGeometry,
  Group,
  LineBasicMaterial,
  LineDashedMaterial,
  LineSegments,
  Mesh,
  MeshBasicMaterial,
  Scene,
  Sprite,
} from "three";
import type { Material, Object3D } from "three";
import type { BoardInfo } from "../../protocol";
import type { GcodeGeometry, GcodeLabel } from "./gcodeGeometry";
import { bedFrame, chainSegments, crosshair, cutCellTriangles, guideSegments } from "./shapes";
import type { MachineGeometry, Point, Point3, Segments } from "./shapes";

/** Scene colours, tuned for the dark theme (theme.css tokens where one fits). */
export const COLORS = {
  background: 0x14171c,
  frame: 0x9aa3b0,
  grid: 0x4a5260,
  chain: 0x6b7480,
  sled: 0xe5534b,
  computedSled: 0x9aa3b0,
  home: 0x3fb67a,
  gcodePosition: 0xe8eaed,
  cursor: 0x4fa3ff,
  guide: 0x2c4a6b,
  cut: 0x4fa3ff,
  rapid: 0x3fb67a,
  plunge: 0x3fb67a,
  event: 0xe5534b,
  boardFill: 0xd2b48c,
  boardEdge: 0xb5651d,
  cutCell: 0xff6666,
} as const;

export type MakeLabel = (text: string) => Object3D;

export interface ViewerScene {
  readonly root: Scene;
  setMachine(machine: MachineGeometry): void;
  /** Sled position in scene inches, or null before the first report. */
  setSled(position: Point3 | null): void;
  setComputedSled(position: Point | null): void;
  /** Home in scene inches; the toolpath and gcode position are drawn relative to it. */
  setHome(position: Point): void;
  setGcodePosition(position: Point3 | null): void;
  setGcode(geometry: GcodeGeometry | null): void;
  setBoard(board: BoardInfo | null, cells: readonly unknown[] | null): void;
  setBoardVisible(visible: boolean): void;
  setLabelsVisible(visible: boolean): void;
  setCursor(position: Point | null): void;
  dispose(): void;
}

const geometryOf = (positions: ArrayLike<number>) =>
  new BufferGeometry().setAttribute(
    "position",
    new BufferAttribute(positions instanceof Float32Array ? positions : Float32Array.from(positions), 3),
  );

/** Swaps an object's geometry and frees the old GPU buffers. */
const replaceGeometry = (object: LineSegments | Mesh, positions: ArrayLike<number>) => {
  const previous = object.geometry;
  object.geometry = geometryOf(positions);
  if (object instanceof LineSegments && object.material instanceof LineDashedMaterial) object.computeLineDistances();
  previous.dispose();
};

const writePositions = (object: LineSegments, segments: Segments) => {
  const attribute = object.geometry.getAttribute("position") as BufferAttribute;
  attribute.set(segments);
  attribute.needsUpdate = true;
  object.geometry.computeBoundingSphere();
};

const materialsOf = (object: Object3D): Material[] =>
  "material" in object ? ([] as Material[]).concat(object.material as Material | Material[]) : [];

/** Frees an object tree's own geometries, materials and textures; Sprite geometry is shared by three, so it stays. */
const disposeTree = (object: Object3D) =>
  object.traverse((child) => {
    if (!(child instanceof Sprite) && "geometry" in child) (child.geometry as BufferGeometry).dispose();
    materialsOf(child).forEach((material) => {
      if ("map" in material) (material.map as { dispose(): void } | null)?.dispose();
      material.dispose();
    });
  });

export const createViewerScene = (makeLabel: MakeLabel): ViewerScene => {
  const line = (color: number) => new LineBasicMaterial({ color });
  const dashed = (color: number, size: number) => new LineDashedMaterial({ color, dashSize: size, gapSize: size });
  const segments = (positions: ArrayLike<number>, material: Material) => {
    const object = new LineSegments(geometryOf(positions), material);
    if (material instanceof LineDashedMaterial) object.computeLineDistances();
    return object;
  };
  const marker = (arm: number, radius: number, color: number, z = 0) => {
    const object = segments(crosshair(arm, radius), line(color));
    object.position.z = z;
    object.visible = false;
    return object;
  };

  const root = new Scene();
  root.background = new Color(COLORS.background);

  const outline = segments([], line(COLORS.frame));
  const grid = segments([], dashed(COLORS.grid, 0.5));
  const chains = segments(new Float32Array(12), line(COLORS.chain));
  chains.visible = false;
  chains.frustumCulled = false;
  const guides = segments(new Float32Array(12), line(COLORS.guide));
  guides.visible = false;
  guides.frustumCulled = false;

  const sled = marker(1.5, 1, COLORS.sled);
  const computedSled = marker(1.5, 1, COLORS.computedSled, -0.01);
  const home = marker(1.25, 0.75, COLORS.home);
  home.visible = true;
  const cursor = marker(1.5, 1, COLORS.cursor);

  const cuts = segments([], line(COLORS.cut));
  const rapids = segments([], dashed(COLORS.rapid, 0.1));
  const plunges = segments([], line(COLORS.plunge));
  const events = segments([], line(COLORS.event));
  const labels = new Group();
  const gcodePosition = marker(1, 0.5, COLORS.gcodePosition);
  const toolpath = new Group().add(cuts, rapids, plunges, events, labels, gcodePosition);

  const boardFill = new Mesh(
    new BoxGeometry(1, 1, 1),
    new MeshBasicMaterial({ color: COLORS.boardFill, opacity: 0.35, transparent: true }),
  );
  const boardEdges = segments([], line(COLORS.boardEdge));
  const cutCells = new Mesh(geometryOf([]), new MeshBasicMaterial({ color: COLORS.cutCell }));
  const board = new Group().add(boardFill, boardEdges);
  const boardLayer = new Group().add(board, cutCells);
  boardLayer.visible = false;

  root.add(boardLayer, outline, grid, guides, chains, toolpath, home, computedSled, sled, cursor);
  Object.entries({
    outline,
    grid,
    chains,
    guides,
    sled,
    computedSled,
    home,
    cursor,
    cuts,
    rapids,
    plunges,
    events,
    labels,
    gcodePosition,
    toolpath,
    board,
    boardFill,
    boardEdges,
    cutCells,
    boardLayer,
  }).forEach(([name, object]) => (object.name = name));

  let machine: MachineGeometry | null = null;
  let sledPosition: Point3 | null = null;
  let labelData: GcodeLabel[] = [];
  let labelsVisible = false;
  let boardVisible = false;
  let hasBoard = false;

  const updateChains = () => {
    chains.visible = machine !== null && sledPosition !== null;
    if (machine && sledPosition) writePositions(chains, chainSegments(machine, sledPosition));
  };

  const clearLabels = () => {
    [...labels.children].forEach((label) => {
      labels.remove(label);
      disposeTree(label);
    });
  };

  const showLabels = () => {
    clearLabels();
    if (!labelsVisible) return;
    labelData.forEach(({ text, position }) => {
      const label = makeLabel(text);
      label.position.set(position.x, position.y, position.z);
      labels.add(label);
    });
  };

  const place = (object: Object3D, position: Point | Point3 | null) => {
    object.visible = position !== null;
    if (position) object.position.set(position.x, position.y, "z" in position ? position.z : object.position.z);
  };

  return {
    root,
    setMachine: (next) => {
      machine = next;
      const frame = bedFrame(next);
      replaceGeometry(outline, frame.outline);
      replaceGeometry(grid, frame.grid);
      updateChains();
    },
    setSled: (position) => {
      sledPosition = position;
      place(sled, position);
      updateChains();
    },
    setComputedSled: (position) => place(computedSled, position),
    setHome: ({ x, y }) => {
      home.position.set(x, y, 0);
      toolpath.position.set(x, y, 0);
    },
    setGcodePosition: (position) => place(gcodePosition, position),
    setGcode: (geometry) => {
      replaceGeometry(cuts, geometry?.cuts ?? []);
      replaceGeometry(rapids, geometry?.rapids ?? []);
      replaceGeometry(plunges, geometry?.plunges ?? []);
      replaceGeometry(events, geometry?.events ?? []);
      labelData = geometry?.labels ?? [];
      showLabels();
    },
    setBoard: (info, cells) => {
      hasBoard = info !== null && info.width > 0 && info.height > 0;
      boardLayer.visible = boardVisible && hasBoard;
      if (!info || !hasBoard) return replaceGeometry(cutCells, []);
      const box = new BoxGeometry(info.width, info.height, info.thickness);
      const previous = boardFill.geometry;
      boardFill.geometry = box;
      previous.dispose();
      const edges = boardEdges.geometry;
      boardEdges.geometry = new EdgesGeometry(box);
      edges.dispose();
      board.position.set(info.centerX, info.centerY, -info.thickness / 2);
      replaceGeometry(cutCells, cells ? cutCellTriangles(info, cells) : []);
    },
    setBoardVisible: (visible) => {
      boardVisible = visible;
      boardLayer.visible = visible && hasBoard;
    },
    setLabelsVisible: (visible) => {
      labelsVisible = visible;
      showLabels();
    },
    setCursor: (position) => {
      place(cursor, position);
      guides.visible = position !== null && machine !== null;
      if (position && machine) writePositions(guides, guideSegments(machine, position));
    },
    dispose: () => {
      clearLabels();
      disposeTree(root);
    },
  };
};
