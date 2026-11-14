import { MOUSE, OrthographicCamera, PerspectiveCamera, TOUCH, WebGLRenderer } from "three";
import type { Scene } from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { pickBedPoint } from "./picking";
import { DEFAULT_MACHINE, fitOrthographic, fitPerspectiveDistance } from "./shapes";
import type { MachineGeometry, Point } from "./shapes";

/** The WebGL half of the viewer: renderer, cameras and orbit controls. Scene contents live in scene.ts. */
export interface ViewerEngine {
  /** Draws on the next animation frame; repeated calls before then draw once. */
  requestRender(): void;
  pick(clientX: number, clientY: number): Point | null;
  setRotate(enabled: boolean): void;
  setPerspective(enabled: boolean): void;
  /** Frames the bed and makes that the view Reset returns to. */
  fit(machine: MachineGeometry): void;
  reset(): void;
  dispose(): void;
}

const FOV = 45;
const CAMERA_Z = 100;

export const createViewerEngine = (host: HTMLElement, scene: Scene): ViewerEngine => {
  const renderer = new WebGLRenderer({ antialias: true });
  renderer.setPixelRatio(window.devicePixelRatio);
  host.appendChild(renderer.domElement);

  const ortho = new OrthographicCamera(-1, 1, 1, -1, 0.1, 2000);
  const perspective = new PerspectiveCamera(FOV, 1, 0.5, 4000);
  let camera: OrthographicCamera | PerspectiveCamera = ortho;
  let machine = DEFAULT_MACHINE;
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
    const { halfWidth, halfHeight } = fitOrthographic(machine, aspect());
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
    ortho.position.set(0, 0, CAMERA_Z);
    perspective.position.set(0, 0, fitPerspectiveDistance(machine, aspect(), FOV));
    [ortho, perspective].forEach((each) => each.up.set(0, 1, 0));
    controls.target.set(0, 0, 0);
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
      machine = next;
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
