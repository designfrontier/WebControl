import { Plane, Raycaster, Vector2, Vector3 } from "three";
import type { Camera } from "three";
import type { Point } from "./shapes";

export interface Rect {
  left: number;
  top: number;
  width: number;
  height: number;
}

const BED_PLANE = new Plane(new Vector3(0, 0, 1), 0);

/** Where a screen point lands on the z = 0 bed plane, in scene inches; null if the ray misses it. */
export const pickBedPoint = (camera: Camera, rect: Rect, clientX: number, clientY: number): Point | null => {
  if (rect.width <= 0 || rect.height <= 0) return null;
  const ndc = new Vector2(((clientX - rect.left) / rect.width) * 2 - 1, -((clientY - rect.top) / rect.height) * 2 + 1);
  const raycaster = new Raycaster();
  raycaster.setFromCamera(ndc, camera);
  const hit = raycaster.ray.intersectPlane(BED_PLANE, new Vector3());
  return hit ? { x: hit.x, y: hit.y } : null;
};
