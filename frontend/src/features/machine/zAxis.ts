import { decimalPoint } from "../../numbers/decimal";
import type { Units } from "../../protocol";

const MM_PER_INCH = 25.4;

/** A Z step the user typed: a positive decimal, or null. */
export const parseDistance = (text: string): number | null => {
  const trimmed = decimalPoint(text.trim());
  if (!/^\d*\.?\d+$|^\d+\.$/.test(trimmed)) return null;
  const value = Number(trimmed);
  return value > 0 ? value : null;
};

/** The updateSetting the old unitSwitchZ sent: the other unit's name and the step converted into it. */
export const unitSwitch = (units: Units, distance: number) =>
  units === "MM"
    ? { setting: "toInchesZ", value: (distance / MM_PER_INCH).toFixed(2) }
    : { setting: "toMMZ", value: (distance * MM_PER_INCH).toFixed(2) };

export const unitLabel = (units: Units) => (units === "MM" ? "mm" : "in");

export const LIMIT_KEYS = { upper: "zAxisUpperLimit", lower: "zAxisLowerLimit" } as const;
export type LimitKey = (typeof LIMIT_KEYS)[keyof typeof LIMIT_KEYS];

/** The reported Z as the limits store it: mm, to the micron. */
export const zInMm = (z: number, units: Units) => String(Number((units === "MM" ? z : z * MM_PER_INCH).toFixed(3)));

export const limitText = (value: string | number | boolean | undefined) =>
  value === undefined || String(value).trim() === "" ? "Not set" : `${String(value)} mm`;
