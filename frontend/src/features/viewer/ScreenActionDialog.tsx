import { useId } from "react";
import type { Units } from "../../protocol";
import type { Point } from "./shapes";
import { fromInches, unitLabel } from "./units";

export type ScreenAction = "moveTo" | "defineHome" | "returnToCenter";

interface Props {
  /** The picked bed point, in scene inches. */
  point: Point;
  units: Units | null;
  onBed: boolean;
  running: boolean;
  onAction(action: ScreenAction): void;
  onClose(): void;
}

/** The right-click / long-press menu on the viewer (old templates/screenAction.html), owned by the client. */
export const ScreenActionDialog = ({ point, units, onBed, running, onAction, onClose }: Props) => {
  const titleId = useId();
  const format = (inches: number) => fromInches(inches, units).toFixed(units === "MM" ? 1 : 2);

  return (
    <div className="viewer-dialog-backdrop" onClick={onClose}>
      <div
        className="viewer-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onClick={(event) => event.stopPropagation()}
        onKeyDown={(event) => event.key === "Escape" && onClose()}
      >
        <h2 id={titleId}>Screen action</h2>
        <p>
          X {format(point.x)}, Y {format(point.y)} {unitLabel(units)}
        </p>
        {running ? <p className="muted">Unavailable while gcode is running.</p> : null}
        {!onBed && !running ? (
          <p className="muted">This point is outside the work area, so the sled cannot move there.</p>
        ) : null}
        <div className="viewer-dialog-actions">
          <button type="button" disabled={running || !onBed} onClick={() => onAction("moveTo")}>
            Move here
          </button>
          <button type="button" disabled={running} onClick={() => onAction("defineHome")}>
            Define home here
          </button>
          <button type="button" disabled={running} onClick={() => onAction("returnToCenter")}>
            Return to center
          </button>
          <button type="button" autoFocus onClick={onClose}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
};
