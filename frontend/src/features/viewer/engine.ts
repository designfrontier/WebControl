import { MOUSE, OrthographicCamera, PerspectiveCamera, TOUCH, WebGLRenderer } from "three";
import type { Scene } from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { pickBedPoint } from "./picking";
import { DEFAULT_MACHINE, bedViewFrame, fitOrthographic, fitPerspectiveDistance } from "./shapes";
import type { Point, ViewFrame } from "./shapes";

/** The WebGL half of the viewer: renderer, cameras and orbit controls. Scene contents live in scene.ts. */
export interface ViewerEngine {
  /** Draws on the next animation frame; repeated calls before then draw once. */
  requestRender(): void;
  pick(clientX: number, clientY: number): Point | null;
  setRotate(enabled: boolean): void;
  setPerspective(enabled: boolean): void;
  /** Frames this area and makes that the view Reset returns to. */
  fit(frame: ViewFrame): void;
  reset(): void;
  dispose(): void;
}

const FOV = 45;
const CAMERA_Z = 100;

/** Throws when the browser has no WebGL 2, which current three.js requires. */
export const createViewerEngine = (host: HTMLElement, scene: Scene): ViewerEngine => {
  if (typeof WebGL2RenderingContext === "undefined") throw new Error("WebGL 2 is not supported");
  const renderer = new WebGLRenderer({ antialias: true });
  renderer.setPixelRatio(window.devicePixelRatio);
  host.appendChild(renderer.domElement);

  const ortho = new OrthographicCamera(-1, 1, 1, -1, 0.1, 2000);
  const perspective = new PerspectiveCamera(FOV, 1, 0.5, 4000);
  let camera: OrthographicCamera | PerspectiveCamera = ortho;
  let view = bedViewFrame(DEFAULT_MACHINE);
  let rotate = false;
  let frame = 0;

  const aspect = () => Math.max(host.clientWidth, 1) / Math.max(host.clientHeight, 1);

  const requestRender = () => {
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      renderer.render(scene, camera);
    });
  };

  const makeControls = () => {
    const next = new OrbitControls(camera, renderer.domElement);
    next.screenSpacePanning = true;
    next.enableRotate = rotate;
    next.mouseButtons = { LEFT: MOUSE.PAN, MIDDLE: MOUSE.DOLLY, RIGHT: MOUSE.ROTATE };
    next.touches = { ONE: rotate ? TOUCH.ROTATE : TOUCH.PAN, TWO: TOUCH.DOLLY_PAN };
    next.addEventListener("change", requestRender);
    return next;
  };
  let controls = makeControls();

  const frameCameras = () => {
    const { halfWidth, halfHeight } = fitOrthographic(view, aspect());
    Object.assign(ortho, { left: -halfWidth, right: halfWidth, top: halfHeight, bottom: -halfHeight });
    ortho.updateProjectionMatrix();
    perspective.aspect = aspect();
    perspective.updateProjectionMatrix();
  };

  const resize = () => {
    renderer.setSize(host.clientWidth, host.clientHeight);
    frameCameras();
    requestRender();
  };
  const observer = new ResizeObserver(resize);
  observer.observe(host);

  const home = () => {
    ortho.zoom = 1;
    ortho.position.set(view.x, view.y, CAMERA_Z);
    perspective.position.set(view.x, view.y, fitPerspectiveDistance(view, aspect(), FOV));
    [ortho, perspective].forEach((each) => each.up.set(0, 1, 0));
    controls.target.set(view.x, view.y, 0);
    frameCameras();
    controls.update();
    controls.saveState();
  };
  resize();
  home();

  return {
    requestRender,
    pick: (clientX, clientY) => {
      camera.updateMatrixWorld();
      return pickBedPoint(camera, renderer.domElement.getBoundingClientRect(), clientX, clientY);
    },
    setRotate: (enabled) => {
      rotate = enabled;
      controls.enableRotate = enabled;
      controls.touches = { ONE: enabled ? TOUCH.ROTATE : TOUCH.PAN, TWO: TOUCH.DOLLY_PAN };
    },
    setPerspective: (enabled) => {
      if ((camera === perspective) === enabled) return;
      controls.dispose();
      camera = enabled ? perspective : ortho;
      controls = makeControls();
      home();
      requestRender();
    },
    fit: (next) => {
      view = next;
      home();
      requestRender();
    },
    reset: () => {
      controls.reset();
      requestRender();
    },
    dispose: () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      controls.dispose();
      renderer.dispose();
      renderer.domElement.remove();
    },
  };
};
