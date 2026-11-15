import { Dialog } from "../../app/Dialog";
import type { Units } from "../../protocol";
import type { RunState } from "../../state/machine";
import type { Point } from "./shapes";
import { fromInches, unitLabel } from "./units";

export type ScreenAction = "moveTo" | "defineHome" | "returnToCenter";

interface Props {
  /** The picked bed point, in scene inches. */
  point: Point;
  units: Units | null;
  onBed: boolean;
  runState: RunState;
  /** selectGcodeLocked: the server refuses defineHome until the job is stopped or a tool change resumed. */
  homeLocked: boolean;
  onAction(action: ScreenAction, point: Point): void;
  onClose(): void;
}

/** The right-click / long-press menu on the viewer (old templates/screenAction.html), owned by the client. */
export const ScreenActionDialog = ({ point, units, onBed, runState, homeLocked, onAction, onClose }: Props) => {
  const running = runState === "running";
  const paused = runState === "paused";
  const format = (inches: number) => fromInches(inches, units).toFixed(units === "MM" ? 1 : 2);

  return (
    <Dialog
      title="Screen action"
      onClose={onClose}
      closeButton={false}
      closeOnBackdrop
      actions={
        <>
          <button type="button" disabled={running || !onBed} onClick={() => onAction("moveTo", point)}>
            Move here
          </button>
          <button
            type="button"
            disabled={running || paused || homeLocked}
            onClick={() => onAction("defineHome", point)}
          >
            Define home here
          </button>
          <button type="button" disabled={running} onClick={() => onAction("returnToCenter", point)}>
            Return to center
          </button>
          <button type="button" data-autofocus onClick={onClose}>
            Cancel
          </button>
        </>
      }
    >
      <p>
        X {format(point.x)}, Y {format(point.y)} {unitLabel(units)}
      </p>
      {running ? <p className="muted">Unavailable while gcode is running.</p> : null}
      {paused ? <p className="muted">Define home is unavailable while a run is paused.</p> : null}
      {!onBed && !running ? (
        <p className="muted">This point is outside the work area, so the sled cannot move there.</p>
      ) : null}
    </Dialog>
  );
};
