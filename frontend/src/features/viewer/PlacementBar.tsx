import type { Units } from "../../protocol";
import type { RunState } from "../../state/machine";
import type { Point } from "./shapes";
import { fromInches, unitLabel } from "./units";

interface Props {
  /** The home the placed toolpath needs, in scene inches. */
  home: Point;
  units: Units | null;
  fits: boolean;
  runState: RunState;
  homeLocked: boolean;
  probing: boolean;
  onConfirm(): void;
  onCancel(): void;
}

/** Why Define home would be refused now, as the screen action menu says it; null when it would not. */
const blockedReason = ({ runState, homeLocked, probing }: Pick<Props, "runState" | "homeLocked" | "probing">) =>
  probing
    ? "Unavailable while the probe runs."
    : runState === "running"
      ? "Unavailable while gcode is running."
      : runState === "paused"
        ? "Setting home is unavailable while a run is paused."
        : homeLocked
          ? "Stop the job before setting home."
          : null;

export const PlacementBar = ({ home, units, fits, onConfirm, onCancel, ...gates }: Props) => {
  const format = (inches: number) => fromInches(inches, units).toFixed(units === "MM" ? 1 : 2);
  const blocked = blockedReason(gates);
  return (
    <div className="viewer-placement" role="group" aria-label="Position gcode">
      <span>
        Drag the toolpath into place. Home X {format(home.x)}, Y {format(home.y)} {unitLabel(units)}
      </span>
      {fits ? null : <span role="alert">Part of the toolpath is off the bed.</span>}
      {blocked ? <span className="muted">{blocked}</span> : null}
      <button type="button" disabled={blocked !== null} onClick={onConfirm}>
        Set home here
      </button>
      <button type="button" onClick={onCancel}>
        Cancel
      </button>
    </div>
  );
};
