import { CanvasTexture, Group, Sprite } from "three";
import type { SpriteMaterial } from "three";
import { describe, expect, it, vi } from "vitest";
import { createTextLabel } from "./labels";

const fakeContext = () => ({
  font: "",
  fillStyle: "",
  textBaseline: "",
  measureText: vi.fn(() => ({ width: 92 })),
  fillText: vi.fn(),
});

describe("createTextLabel", () => {
  it("draws the text onto a canvas texture sprite a quarter inch tall", () => {
    const context = fakeContext();
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(context as unknown as CanvasRenderingContext2D);

    const label = createTextLabel("S12000") as Sprite;

    expect(label).toBeInstanceOf(Sprite);
    expect(context.fillText).toHaveBeenCalledWith("S12000", 4, 28);
    expect(context.fillStyle).toBe("#e5534b");
    expect((label.material as SpriteMaterial).map).toBeInstanceOf(CanvasTexture);
    expect(label.scale.y).toBe(0.25);
    expect(label.scale.x).toBeCloseTo((0.25 * 100) / 56);
  });

  it("falls back to an empty object without a 2D canvas", () => {
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
    expect(createTextLabel("T1")).toBeInstanceOf(Group);
  });
});
