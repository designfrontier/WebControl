import { Group, Sprite, SpriteMaterial, Texture } from "three";
import type { BoxGeometry, BufferAttribute, BufferGeometry, LineSegments, Mesh, Object3D } from "three";
import { describe, expect, it, vi } from "vitest";
import { buildGcodeGeometry } from "./gcodeGeometry";
import type { GcodeGeometry } from "./gcodeGeometry";
import { createViewerScene } from "./scene";
import { DEFAULT_MACHINE } from "./shapes";

const board = {
  width: 24,
  height: 12,
  thickness: 0.5,
  centerX: 3,
  centerY: -2,
  boardID: "b1",
  material: "Ply",
  fileName: "",
};

const setup = () => {
  const makeLabel = vi.fn((text: string) => Object.assign(new Group(), { name: `label:${text}` }));
  const scene = createViewerScene(makeLabel);
  const get = <T extends Object3D = LineSegments>(name: string) => scene.root.getObjectByName(name) as T;
  const vertices = (name: string) => get<LineSegments | Mesh>(name).geometry.getAttribute("position").count;
  return { scene, makeLabel, get, vertices };
};

const toolpath = (): GcodeGeometry =>
  buildGcodeGeometry([
    {
      type: "line",
      points: [
        [0, 0, 0],
        [1, 0, -0.1],
      ],
      dashed: false,
      command: null,
    },
    {
      type: "line",
      points: [
        [1, 0, 0],
        [2, 2, 0],
      ],
      dashed: true,
      command: null,
    },
    {
      type: "circle",
      points: [
        [1, 1, 0],
        [3, 12000, 0],
      ],
      dashed: false,
      command: "SpindleOnCW",
    },
    {
      type: "circle",
      points: [
        [2, 2, 0],
        [6, 3, 0],
      ],
      dashed: false,
      command: "ToolChange",
    },
  ]);

describe("createViewerScene", () => {
  it("sizes the bed frame to the machine and frees the old frame", () => {
    const { scene, get, vertices } = setup();
    const before = get("outline").geometry;
    const dispose = vi.spyOn(before, "dispose");

    scene.setMachine(DEFAULT_MACHINE);

    expect(dispose).toHaveBeenCalled();
    expect(vertices("outline")).toBe(8);
    expect(vertices("grid")).toBe(8);
    expect(get("grid").geometry.getAttribute("lineDistance")).toBeDefined();
  });

  it("draws the whole toolpath as four buffers and disposes them on every update", () => {
    const { scene, get, vertices } = setup();
    scene.setGcode(toolpath());
    expect(vertices("cuts")).toBe(2 + 2 + 2);
    expect(vertices("rapids")).toBe(2);
    expect(get("rapids").geometry.getAttribute("lineDistance")).toBeDefined();
    expect(vertices("events")).toBeGreaterThan(0);

    const previous = ["cuts", "rapids", "plunges", "events"].map((name) => vi.spyOn(get(name).geometry, "dispose"));
    scene.setGcode(null);

    previous.forEach((dispose) => expect(dispose).toHaveBeenCalledOnce());
    expect(["cuts", "rapids", "plunges", "events"].map(vertices)).toEqual([0, 0, 0, 0]);
  });

  it("uploads the toolpath buffers without copying them", () => {
    const { scene, get } = setup();
    const geometry = toolpath();
    scene.setGcode(geometry);
    expect(get("cuts").geometry.getAttribute("position").array).toBe(geometry.cuts);
  });

  it("creates labels only while they are shown and frees them when the gcode changes", () => {
    const { scene, makeLabel, get } = setup();
    scene.setGcode(toolpath());
    expect(makeLabel).not.toHaveBeenCalled();

    scene.setLabelsVisible(true);
    expect(get<Group>("labels").children.map((label) => label.name)).toEqual(["label:S12000", "label:T3"]);
    expect(get<Group>("labels").children[0].position.toArray()).toEqual([1, 1, 0]);

    scene.setGcode(null);
    expect(get<Group>("labels").children).toEqual([]);

    scene.setGcode(toolpath());
    scene.setLabelsVisible(false);
    expect(get<Group>("labels").children).toEqual([]);
  });

  it("disposes sprite label textures and materials but not three's shared sprite geometry", () => {
    const texture = new Texture();
    const material = new SpriteMaterial({ map: texture });
    const sprite = new Sprite(material);
    const spies = [vi.spyOn(texture, "dispose"), vi.spyOn(material, "dispose")];
    const geometry = vi.spyOn(sprite.geometry, "dispose");
    const scene = createViewerScene(() => sprite);
    scene.setGcode(toolpath());
    scene.setLabelsVisible(true);
    scene.setLabelsVisible(false);
    spies.forEach((spy) => expect(spy).toHaveBeenCalled());
    expect(geometry).not.toHaveBeenCalled();
  });

  it("draws the last run's cuts at their own home until cleared", () => {
    const { scene, get } = setup();
    expect(get("ghost").visible).toBe(false);
    scene.setGhost({ cuts: new Float32Array([0, 0, 0, 1, 0, 0]), home: { x: -4, y: 3 } });
    expect(get("ghost").visible).toBe(true);
    expect(get("ghost").position.toArray()).toEqual([-4, 3, 0]);
    scene.setHome({ x: 10, y: -5 });
    expect(get("ghost").position.toArray()).toEqual([-4, 3, 0]);
    scene.setGhost(null);
    expect(get("ghost").visible).toBe(false);
  });

  it("places the toolpath and gcode position relative to home", () => {
    const { scene, get } = setup();
    scene.setHome({ x: 10, y: -5 });
    scene.setGcodePosition({ x: 1, y: 2, z: -0.25 });
    expect(get("home").position.toArray()).toEqual([10, -5, 0]);
    expect(get<Group>("toolpath").position.toArray()).toEqual([10, -5, 0]);
    expect(get("gcodePosition").position.toArray()).toEqual([1, 2, -0.25]);
    expect(get("gcodePosition").visible).toBe(true);
    scene.setGcodePosition(null);
    expect(get("gcodePosition").visible).toBe(false);
  });

  it("shows the sled with chains to both motors once the machine and position are known", () => {
    const { scene, get } = setup();
    scene.setSled({ x: 4, y: 5, z: 0 });
    expect(get("sled").visible).toBe(true);
    expect(get("chains").visible).toBe(false);

    scene.setMachine(DEFAULT_MACHINE);
    expect(get("chains").visible).toBe(true);
    const chain = get("chains").geometry.getAttribute("position") as BufferAttribute;
    expect([chain.getX(1), chain.getY(1)]).toEqual([4, 5]);
    expect(chain.getX(0)).toBeCloseTo(-58.6299);

    scene.setSled(null);
    expect(get("sled").visible).toBe(false);
    expect(get("chains").visible).toBe(false);
  });

  it("shows the computed sled at its xy, just under the bed plane", () => {
    const { scene, get } = setup();
    scene.setComputedSled({ x: 1, y: 2 });
    expect(get("computedSled").position.toArray()).toEqual([1, 2, -0.01]);
    scene.setComputedSled(null);
    expect(get("computedSled").visible).toBe(false);
  });

  it("moves the cursor and its guides, which need the bed size", () => {
    const { scene, get } = setup();
    scene.setCursor({ x: 200, y: 1 });
    expect(get("cursor").visible).toBe(true);
    expect(get("guides").visible).toBe(false);

    scene.setMachine(DEFAULT_MACHINE);
    scene.setCursor({ x: 200, y: 1 });
    expect(get("guides").visible).toBe(true);
    expect((get("guides").geometry.getAttribute("position") as BufferAttribute).getX(0)).toBe(48);

    scene.setCursor(null);
    expect(get("cursor").visible).toBe(false);
    expect(get("guides").visible).toBe(false);
  });

  it("draws the board and its cut cells only while the board is toggled on", () => {
    const { scene, get, vertices } = setup();
    scene.setBoard(board, [true, false, true]);
    expect(get<Group>("boardLayer").visible).toBe(false);

    scene.setBoardVisible(true);
    expect(get<Group>("boardLayer").visible).toBe(true);
    expect(get<Group>("board").position.toArray()).toEqual([3, -2, -0.25]);
    expect((get<Mesh>("boardFill").geometry as BoxGeometry).parameters).toMatchObject({
      width: 24,
      height: 12,
      depth: 0.5,
    });
    expect(vertices("cutCells")).toBe(12);

    scene.setBoard(board, null);
    expect(vertices("cutCells")).toBe(0);
    scene.setBoard(null, [true]);
    expect(get<Group>("boardLayer").visible).toBe(false);
    scene.setBoard({ ...board, width: 0 }, null);
    expect(get<Group>("boardLayer").visible).toBe(false);
  });

  it("disposes every geometry and material it owns", () => {
    const { scene, get } = setup();
    scene.setGcode(toolpath());
    scene.setLabelsVisible(true);
    const geometry = vi.spyOn(get("cuts").geometry as BufferGeometry, "dispose");
    const material = vi.spyOn(get<Mesh>("boardFill").material as { dispose(): void }, "dispose");
    scene.dispose();
    expect(geometry).toHaveBeenCalled();
    expect(material).toHaveBeenCalled();
    expect(get<Group>("labels").children).toEqual([]);
  });
});
