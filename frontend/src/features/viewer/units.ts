import type { Units } from "../../protocol";

export const MM_PER_INCH = 25.4;

/** The scene is drawn in inches; positions arrive in the machine's current units. */
export const toInches = (value: number, units: Units | null) => (units === "MM" ? value / MM_PER_INCH : value);

export const fromInches = (value: number, units: Units | null) => (units === "MM" ? value * MM_PER_INCH : value);

export const unitLabel = (units: Units | null) => (units === "MM" ? "mm" : "in");

/** Actions.moveTo and defineHome take scene inches; the old screenAction modal sent 4 decimals. */
export const roundForAction = (value: number) => Number(value.toFixed(4));
