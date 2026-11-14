import { OrthographicCamera, PerspectiveCamera } from "three";
import { describe, expect, it } from "vitest";
import { pickBedPoint } from "./picking";

const rect = { left: 10, top: 20, width: 200, height: 100 };

const ortho = () => {
  const camera = new OrthographicCamera(-50, 50, 25, -25, 1, 1000);
  camera.position.set(0, 0, 100);
  camera.lookAt(0, 0, 0);
  camera.updateMatrixWorld();
  return camera;
};

describe("pickBedPoint", () => {
  it("maps the centre and corners of an orthographic view onto the bed", () => {
    const camera = ortho();
    const centre = pickBedPoint(camera, rect, 110, 70)!;
    expect(centre.x).toBeCloseTo(0);
    expect(centre.y).toBeCloseTo(0);
    const topLeft = pickBedPoint(camera, rect, 10, 20)!;
    expect(topLeft.x).toBeCloseTo(-50);
    expect(topLeft.y).toBeCloseTo(25);
  });

  it("follows a panned orthographic camera", () => {
    const camera = ortho();
    camera.position.set(12, -3, 100);
    camera.updateMatrixWorld();
    const point = pickBedPoint(camera, rect, 110, 70)!;
    expect(point.x).toBeCloseTo(12);
    expect(point.y).toBeCloseTo(-3);
  });

  it("works through a perspective camera", () => {
    const camera = new PerspectiveCamera(90, 2, 1, 500);
    camera.position.set(0, 0, 10);
    camera.lookAt(0, 0, 0);
    camera.updateMatrixWorld();
    const right = pickBedPoint(camera, rect, 210, 70)!;
    expect(right.x).toBeCloseTo(20);
    expect(right.y).toBeCloseTo(0);
  });

  it("returns null when the ray never reaches the bed or the view has no size", () => {
    const camera = new PerspectiveCamera(45, 2, 1, 500);
    camera.position.set(0, 0, 10);
    camera.lookAt(0, 10, 10);
    camera.updateMatrixWorld();
    expect(pickBedPoint(camera, rect, 110, 20)).toBeNull();
    expect(pickBedPoint(ortho(), { ...rect, width: 0 }, 0, 0)).toBeNull();
  });
});
