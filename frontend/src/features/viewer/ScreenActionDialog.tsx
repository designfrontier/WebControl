import { useEffect, useEffectEvent, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";
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
  onAction(action: ScreenAction, point: Point): void;
  onClose(): void;
}

/** Marks every other child of <body> inert and returns the undo. */
const inertSiblings = (keep: Element) => {
  const others = Array.from(document.body.children).filter((child) => child !== keep && !child.hasAttribute("inert"));
  others.forEach((child) => child.setAttribute("inert", ""));
  return () => others.forEach((child) => child.removeAttribute("inert"));
};

/** Wraps Tab from either end of `container`, or pulls focus back in from outside it. */
const trapTab = (event: KeyboardEvent, container: HTMLElement) => {
  const buttons = Array.from(container.querySelectorAll<HTMLButtonElement>("button:not(:disabled)"));
  const inside = container.contains(document.activeElement);
  const [first, last] = [buttons[0], buttons[buttons.length - 1]];
  const target = event.shiftKey
    ? !inside || document.activeElement === first
      ? last
      : null
    : !inside || document.activeElement === last
      ? first
      : null;
  if (!target) return;
  event.preventDefault();
  target.focus();
};

/** The right-click / long-press menu on the viewer (old templates/screenAction.html), owned by the client. */
export const ScreenActionDialog = ({ point, units, onBed, runState, onAction, onClose }: Props) => {
  const titleId = useId();
  const backdrop = useRef<HTMLDivElement>(null);
  const dialog = useRef<HTMLDivElement>(null);
  const cancel = useRef<HTMLButtonElement>(null);
  // Read during render: once the page behind goes inert, browsers move focus to <body>.
  const [opener] = useState(() => document.activeElement);
  const running = runState === "running";
  const paused = runState === "paused";
  const format = (inches: number) => fromInches(inches, units).toFixed(units === "MM" ? 1 : 2);

  useEffect(() => {
    const restore = backdrop.current ? inertSiblings(backdrop.current) : () => {};
    cancel.current?.focus();
    return () => {
      restore();
      if (opener instanceof HTMLElement && opener.isConnected) opener.focus();
    };
  }, [opener]);

  const onKeyDown = useEffectEvent((event: KeyboardEvent) => {
    if (event.key === "Escape") onClose();
    if (event.key === "Tab" && dialog.current) trapTab(event, dialog.current);
  });

  useEffect(() => {
    const listener = (event: KeyboardEvent) => onKeyDown(event);
    document.addEventListener("keydown", listener);
    return () => document.removeEventListener("keydown", listener);
  }, []);

  return createPortal(
    <div ref={backdrop} className="viewer-dialog-backdrop" onClick={onClose}>
      <div
        ref={dialog}
        className="viewer-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onClick={(event) => event.stopPropagation()}
      >
        <h2 id={titleId}>Screen action</h2>
        <p>
          X {format(point.x)}, Y {format(point.y)} {unitLabel(units)}
        </p>
        {running ? <p className="muted">Unavailable while gcode is running.</p> : null}
        {paused ? <p className="muted">Define home is unavailable while a run is paused.</p> : null}
        {!onBed && !running ? (
          <p className="muted">This point is outside the work area, so the sled cannot move there.</p>
        ) : null}
        <div className="viewer-dialog-actions">
          <button type="button" disabled={running || !onBed} onClick={() => onAction("moveTo", point)}>
            Move here
          </button>
          <button type="button" disabled={running || paused} onClick={() => onAction("defineHome", point)}>
            Define home here
          </button>
          <button type="button" disabled={running} onClick={() => onAction("returnToCenter", point)}>
            Return to center
          </button>
          <button ref={cancel} type="button" onClick={onClose}>
            Cancel
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
};
