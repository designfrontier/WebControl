import type { ReactNode } from "react";
import { useMachine } from "../../app/context";
import { selectJobState } from "../../state/machine";
import { fixed, positioningModeLabel } from "./controls";

const Row = ({ term, wide = false, children }: { term: string; wide?: boolean; children: ReactNode }) => (
  <div className={wide ? "wide" : undefined}>
    <dt>{term}</dt>
    <dd>{children}</dd>
  </div>
);

/** The classic readout block: line, sled, chain error, home, gcode target, progress, tool and positioning mode. */
export const Readouts = ({ lineIndex }: { lineIndex: number | null }) => {
  const position = useMachine((s) => s.position);
  const home = useMachine((s) => s.homePosition);
  const gcode = useMachine((s) => s.gcodePosition);
  const status = useMachine((s) => s.status);
  const error = useMachine((s) => s.positionError);
  const job = useMachine(selectJobState);

  return (
    <dl className="readouts" aria-label="Machine readouts">
      <Row term="Job" wide>
        <span className="job-state" data-state={job ?? "unknown"}>
          {job ?? "—"}
        </span>
      </Row>
      <Row term="Line" wide>
        {lineIndex === null ? "—" : lineIndex + 1}
      </Row>
      <Row term="Sled" wide>
        {position ? `X ${fixed(position.x)} Y ${fixed(position.y)} Z ${fixed(position.z)}` : "—"}
      </Row>
      <Row term="Error" wide>
        <meter aria-label="Left chain error" min={0} max={100} high={50} value={error ? error.left * 100 : 0} />
        <meter aria-label="Right chain error" min={0} max={100} high={50} value={error ? error.right * 100 : 0} />
      </Row>
      <Row term="Home" wide>
        {home ? `X ${fixed(home.x)} Y ${fixed(home.y)}` : "—"}
      </Row>
      <Row term="GCode" wide>
        {gcode ? `X ${fixed(gcode.x)} Y ${fixed(gcode.y)} Z ${fixed(gcode.z)}` : "—"}
      </Row>
      <Row term="Complete">{position?.percentComplete || "—"}</Row>
      <Row term="State">{position?.state || "—"}</Row>
      <Row term="Tool">{status ? status.currentTool : "—"}</Row>
      <Row term="Positioning">{status ? positioningModeLabel(status.positioningMode) : "—"}</Row>
    </dl>
  );
};
