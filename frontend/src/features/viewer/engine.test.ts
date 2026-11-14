import { OrthographicCamera, PerspectiveCamera, Scene, TOUCH } from "three";
import type { Camera } from "three";
import type * as ThreeModule from "three";
import type * as OrbitModule from "three/addons/controls/OrbitControls.js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createViewerEngine } from "./engine";
import { DEFAULT_MACHINE, bedViewFrame } from "./shapes";

const renderer = vi.hoisted(() => ({
  instances: [] as {
    domElement: HTMLCanvasElement;
    render: ReturnType<typeof vi.fn>;
    setSize: ReturnType<typeof vi.fn>;
    dispose: ReturnType<typeof vi.fn>;
  }[],
}));

const controls = vi.hoisted(() => ({ instances: [] as OrbitModule.OrbitControls[] }));

vi.mock("three/addons/controls/OrbitControls.js", async (importOriginal) => {
  const original = await importOriginal<typeof OrbitModule>();
  class TrackedControls extends original.OrbitControls {
    constructor(...args: ConstructorParameters<typeof original.OrbitControls>) {
      super(...args);
      controls.instances.push(this);
    }
  }
  return { OrbitControls: TrackedControls };
});

vi.mock("three", async (importOriginal) => {
  const three = await importOriginal<typeof ThreeModule>();
  class FakeWebGLRenderer {
    domElement = document.createElement("canvas");
    render = vi.fn();
    setSize = vi.fn();
    setPixelRatio = vi.fn();
    dispose = vi.fn();
    constructor() {
      renderer.instances.push(this);
    }
  }
  return { ...three, WebGLRenderer: FakeWebGLRenderer };
});

let resize: () => void = () => {};
const observer = { observe: vi.fn(), disconnect: vi.fn() };

beforeEach(() => {
  renderer.instances.length = 0;
  controls.instances.length = 0;
  vi.stubGlobal(
    "ResizeObserver",
    vi.fn(function (callback: () => void) {
      resize = callback;
      return observer;
    }),
  );
  vi.stubGlobal(
    "requestAnimationFrame",
    vi.fn((callback: FrameRequestCallback) => setTimeout(() => callback(0), 0) as unknown as number),
  );
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
  vi.stubGlobal("WebGL2RenderingContext", class {});
});

afterEach(() => {
  document.body.innerHTML = "";
});

const setup = (width = 800, height = 400) => {
  const host = document.createElement("div");
  Object.defineProperties(host, {
    clientWidth: { value: width, writable: true },
    clientHeight: { value: height, writable: true },
  });
  document.body.append(host);
  const engine = createViewerEngine(host, new Scene());
  const fake = renderer.instances[0];
  vi.spyOn(fake.domElement, "getBoundingClientRect").mockReturnValue(DOMRect.fromRect({ width, height }));
  const flush = () => new Promise((resolve) => setTimeout(resolve, 0));
  const lastCamera = () => fake.render.mock.calls.at(-1)![1] as Camera;
  return { host, engine, fake, flush, lastCamera };
};

describe("createViewerEngine", () => {
  it("mounts a canvas sized to its host and draws once per frame however often asked", async () => {
    const { host, engine, fake, flush } = setup();
    expect(host.querySelector("canvas")).toBe(fake.domElement);
    expect(fake.setSize).toHaveBeenCalledWith(800, 400);
    engine.requestRender();
    engine.requestRender();
    await flush();
    expect(fake.render).toHaveBeenCalledTimes(1);
  });

  it("frames the whole bed in an orthographic top view", async () => {
    const { engine, flush, lastCamera } = setup();
    engine.fit(bedViewFrame(DEFAULT_MACHINE));
    await flush();
    const camera = lastCamera() as OrthographicCamera;
    expect(camera).toBeInstanceOf(OrthographicCamera);
    expect(camera.right).toBeCloseTo(48 * 1.08);
    expect(engine.pick(800, 200)!.x).toBeCloseTo(48 * 1.08);
    expect(engine.pick(400, 200)!.y).toBeCloseTo(0);
  });

  it("refits the cameras when the host resizes", () => {
    const { host, engine, fake } = setup();
    Object.assign(host, { clientHeight: 800 });
    resize();
    expect(fake.setSize).toHaveBeenLastCalledWith(800, 800);
    expect(engine.pick(400, 0)!.y).toBeCloseTo(48 * 1.08);
  });

  it("switches to a perspective camera that shows the same bed", async () => {
    const { engine, flush, lastCamera } = setup();
    engine.setPerspective(true);
    engine.setPerspective(true);
    await flush();
    expect(lastCamera()).toBeInstanceOf(PerspectiveCamera);
    expect(engine.pick(400, 200)!.x).toBeCloseTo(0);
    engine.setPerspective(false);
    await flush();
    expect(lastCamera()).toBeInstanceOf(OrthographicCamera);
  });

  it("only rotates in 3D mode, where one finger rotates instead of panning", () => {
    const { engine } = setup();
    const current = () => controls.instances.at(-1)!;
    expect(current()).toMatchObject({ enableRotate: false, screenSpacePanning: true, touches: { ONE: TOUCH.PAN } });

    engine.setRotate(true);
    expect(current()).toMatchObject({ enableRotate: true, touches: { ONE: TOUCH.ROTATE } });

    engine.setPerspective(true);
    expect(controls.instances).toHaveLength(2);
    expect(current()).toMatchObject({ enableRotate: true, touches: { ONE: TOUCH.ROTATE } });

    engine.setRotate(false);
    expect(current()).toMatchObject({ enableRotate: false, touches: { ONE: TOUCH.PAN } });
  });

  it("returns to the fitted view on reset", async () => {
    const { engine, flush, lastCamera } = setup();
    const camera = () => lastCamera() as OrthographicCamera;
    engine.requestRender();
    await flush();
    camera().position.set(30, 10, 100);
    camera().zoom = 3;
    engine.reset();
    await flush();
    const [x, y, z] = camera().position.toArray();
    expect([x, y, z].map((value) => Math.round(value * 1e6) / 1e6)).toEqual([0, 0, 100]);
    expect(camera().zoom).toBe(1);
  });

  it("refuses to start without WebGL 2", () => {
    vi.stubGlobal("WebGL2RenderingContext", undefined);
    expect(() => createViewerEngine(document.createElement("div"), new Scene())).toThrow("WebGL 2 is not supported");
    expect(renderer.instances).toEqual([]);
  });

  it("releases WebGL, the observer and the canvas on dispose", () => {
    const { host, engine, fake } = setup();
    engine.dispose();
    expect(fake.dispose).toHaveBeenCalled();
    expect(observer.disconnect).toHaveBeenCalled();
    expect(host.querySelector("canvas")).toBeNull();
  });
});
