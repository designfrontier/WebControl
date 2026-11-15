import type { Units } from "../../protocol";
import type { RunState } from "../../state/machine";

export interface ControlAvailability {
  /** Jog, home, macros, Z moves: everything the server refuses while gcode streams. */
  manual: boolean;
  /** The unit switches, which the Rust backend refuses while the gcode is locked. */
  units: boolean;
  /** Set Home Here, which processAction refuses while paused as well as while running. */
  defineHome: boolean;
  play: boolean;
  pause: boolean;
  /** Gcode line stepping, which the server allows while paused but not while running. */
  step: boolean;
}

const CONTROLS: Record<RunState, Omit<ControlAvailability, "units">> = {
  idle: { manual: true, defineHome: true, play: true, pause: false, step: true },
  running: { manual: false, defineHome: false, play: false, pause: true, step: false },
  paused: { manual: true, defineHome: false, play: false, pause: true, step: true },
  unknown: { manual: false, defineHome: false, play: false, pause: false, step: false },
};

/**
 * The old UI's disabler / gcdisabler / ndisabler classes, keyed by uploadFlag. Stop is always available.
 * `gcodeLocked` is selectGcodeLocked: home and the unit switches also wait for a tool change outside a job.
 */
export const controlsFor = (runState: RunState, gcodeLocked = false): ControlAvailability => ({
  ...CONTROLS[runState],
  units: CONTROLS[runState].manual && !gcodeLocked,
  defineHome: CONTROLS[runState].defineHome && !gcodeLocked,
});

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
