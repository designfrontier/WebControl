import type { Units } from "../../protocol";
import type { RunState } from "../../state/machine";

export interface ControlAvailability {
  /** Jog, home, macros, Z moves, units: everything the server refuses while gcode streams. */
  manual: boolean;
  play: boolean;
  pause: boolean;
  /** Gcode line stepping, which the server allows while paused but not while running. */
  step: boolean;
}

/** The old UI's disabler / gcdisabler / ndisabler classes, keyed by uploadFlag. Stop is always available. */
export const controlsFor = (runState: RunState): ControlAvailability => {
  switch (runState) {
    case "idle":
      return { manual: true, play: true, pause: false, step: true };
    case "running":
      return { manual: false, play: false, pause: true, step: false };
    case "paused":
      return { manual: true, play: false, pause: true, step: true };
    case "unknown":
      return { manual: false, play: false, pause: false, step: false };
  }
};

const MM_PER_INCH = 25.4;

/** Parses a jog distance the way the old inputs' pattern did: digits with at most `decimals` decimal places. */
export const parseDistance = (text: string, decimals: number): number | null => {
  const trimmed = text.trim();
  const pattern = new RegExp(`^(\\d+(\\.\\d{0,${decimals}})?|\\.\\d{1,${decimals}})$`);
  return pattern.test(trimmed) ? Number(trimmed) : null;
};

/** The distance re-expressed in the other units, rounded like the old unitSwitch (3 places XY, 2 for Z). */
export const convertDistance = (distance: number, from: Units, decimals: number): string =>
  (from === "MM" ? distance / MM_PER_INCH : distance * MM_PER_INCH).toFixed(decimals);

export const unitsLabel = (units: Units) => (units === "MM" ? "mm" : "in");

export const fixed = (value: number) => value.toFixed(2);

export const positioningModeLabel = (mode: number) => (mode === 0 ? "Absolute (G90)" : "Incremental (G91)");

/** Unwraps alert text the server double-encodes with json.dumps; anything else is shown as sent. */
export const unwrapMessage = (message: string): string => {
  try {
    const parsed: unknown = JSON.parse(message);
    return typeof parsed === "string" ? parsed : message;
  } catch {
    return message;
  }
};
