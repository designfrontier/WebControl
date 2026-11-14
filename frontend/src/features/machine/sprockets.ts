export type MotorId = "L" | "R" | "M" | "S";
export type Direction = "clockwise" | "counterclockwise";

export interface Motor {
  id: MotorId;
  label: string;
  /** The sign setSprockets needs for a clockwise turn; right-side motors are mirrored. */
  clockwiseSign: 1 | -1;
}

const LEFT: Motor = { id: "L", label: "Left motor", clockwiseSign: 1 };
const RIGHT: Motor = { id: "R", label: "Right motor", clockwiseSign: -1 };
const BOTTOM_LEFT: Motor = { id: "M", label: "Bottom left motor", clockwiseSign: 1 };
const BOTTOM_RIGHT: Motor = { id: "S", label: "Bottom right motor", clockwiseSign: -1 };

export const motorsFor = (fourMotor: boolean): Motor[] =>
  fourMotor ? [LEFT, RIGHT, BOTTOM_LEFT, BOTTOM_RIGHT] : [LEFT, RIGHT];

export const SPROCKET_STEPS = [360, 10, 1, 0.1] as const;
export const DIRECTIONS: Direction[] = ["clockwise", "counterclockwise"];

export const sprocketDegrees = (motor: Motor, direction: Direction, step: number) =>
  (direction === "clockwise" ? 1 : -1) * motor.clockwiseSign * step;

export const directionLabel = (direction: Direction) => (direction === "clockwise" ? "Clockwise" : "Counter-clockwise");

/** Seconds the server counts down in adjustChain before it starts feeding chain. */
export const EXTEND_COUNTDOWN_SECONDS = 6;
