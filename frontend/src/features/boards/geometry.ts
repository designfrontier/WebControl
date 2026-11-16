import { decimalPoint } from "../../numbers/decimal";
import type { Point } from "../../state/machine";

export type Vertical = "top" | "middle" | "bottom";
export type Horizontal = "left" | "center" | "right";

export interface Alignment {
  label: string;
  vertical: Vertical | null;
  horizontal: Horizontal | null;
}

/** The 3x3 grid of "put this part of the board under the router" buttons, in reading order. */
export const ALIGNMENTS: Alignment[] = [
  { label: "Top left", vertical: "top", horizontal: "left" },
  { label: "Top", vertical: "top", horizontal: null },
  { label: "Top right", vertical: "top", horizontal: "right" },
  { label: "Left", vertical: null, horizontal: "left" },
  { label: "Center", vertical: "middle", horizontal: "center" },
  { label: "Right", vertical: null, horizontal: "right" },
  { label: "Bottom left", vertical: "bottom", horizontal: "left" },
  { label: "Bottom", vertical: "bottom", horizontal: null },
  { label: "Bottom right", vertical: "bottom", horizontal: "right" },
];

const OFFSET = { left: 1, center: 0, right: -1, bottom: 1, middle: 0, top: -1 } as const;

/** The board center that puts the chosen edge or corner under the router; null leaves that axis alone. */
export const alignedCenter = (router: Point, size: { width: number; height: number }, alignment: Alignment) => ({
  centerX: alignment.horizontal === null ? null : router.x + (OFFSET[alignment.horizontal] * size.width) / 2,
  centerY: alignment.vertical === null ? null : router.y + (OFFSET[alignment.vertical] * size.height) / 2,
});

/** A finite number from a text field, or null; blank is not zero. */
export const parseNumber = (text: string) => {
  const trimmed = decimalPoint(text.trim());
  const value = Number(trimmed);
  return trimmed !== "" && Number.isFinite(value) ? value : null;
};

export const formatNumber = (value: number) => String(Math.round(value * 100) / 100);
