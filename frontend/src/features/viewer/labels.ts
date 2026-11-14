import { CanvasTexture, Group, LinearFilter, Sprite, SpriteMaterial } from "three";
import type { Object3D } from "three";

const FONT_PX = 48;
/** World height of a label in inches. */
const LABEL_HEIGHT = 0.25;

/** A camera-facing text sprite for spindle-speed and tool-change labels; an empty group if canvas is unavailable. */
export const createTextLabel = (text: string, color = "#e5534b"): Object3D => {
  const canvas = document.createElement("canvas");
  const context = canvas.getContext("2d");
  if (!context) return new Group();
  context.font = `${FONT_PX}px sans-serif`;
  canvas.width = Math.ceil(context.measureText(text).width) + 8;
  canvas.height = FONT_PX + 8;
  context.font = `${FONT_PX}px sans-serif`;
  context.fillStyle = color;
  context.textBaseline = "middle";
  context.fillText(text, 4, canvas.height / 2);
  const texture = new CanvasTexture(canvas);
  texture.minFilter = LinearFilter;
  const sprite = new Sprite(new SpriteMaterial({ map: texture, depthWrite: false }));
  sprite.scale.set((LABEL_HEIGHT * canvas.width) / canvas.height, LABEL_HEIGHT, 1);
  return sprite;
};
