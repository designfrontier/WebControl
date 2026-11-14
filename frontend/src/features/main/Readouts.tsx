import type { ReactNode } from "react";
import { useMachine } from "../../app/context";
import { fixed, positioningModeLabel } from "./controls";

const MOTOR_POWER_MAX = 255;
const MOTORS = [
  ["left", "Left"],
  ["right", "Right"],
  ["z", "Z"],
] as const;

const Row = ({ term, children }: { term: string; children: ReactNode }) => (
  <div>
    <dt>{term}</dt>
    <dd>{children}</dd>
  </div>
);

export const Readouts = () => {
  const position = useMachine((s) => s.position);
  const home = useMachine((s) => s.homePosition);
  const gcode = useMachine((s) => s.gcodePosition);
  const status = useMachine((s) => s.status);
  const error = useMachine((s) => s.positionError);
  const units = useMachine((s) => s.units);

  return (
    <section className="panel" aria-labelledby="readouts-title">
      <h2 id="readouts-title">Position</h2>
      <dl className="readouts">
        <Row term="Sled">{position ? `X ${fixed(position.x)} Y ${fixed(position.y)} Z ${fixed(position.z)}` : "—"}</Row>
        <Row term="Velocity">
          {position ? `${fixed(position.velocity)} / ${fixed(position.requestedVelocity)}` : "—"}
        </Row>
        <Row term="Home">{home ? `X ${fixed(home.x)} Y ${fixed(home.y)}` : "—"}</Row>
        <Row term="Gcode target">{gcode ? `X ${fixed(gcode.x)} Y ${fixed(gcode.y)} Z ${fixed(gcode.z)}` : "—"}</Row>
        <Row term="Complete">{position?.percentComplete || "—"}</Row>
        <Row term="State">{position?.state || "—"}</Row>
        <Row term="Tool">{status ? status.currentTool : "—"}</Row>
        <Row term="Positioning">{status ? positioningModeLabel(status.positioningMode) : "—"}</Row>
        <Row term="Units">{units ?? "—"}</Row>
        {error?.computed && <Row term="Computed">{`X ${fixed(error.computed.x)} Y ${fixed(error.computed.y)}`}</Row>}
      </dl>
      <div className="meters">
        <label>
          Left chain error
          <meter min={0} max={100} high={50} value={error ? error.left * 100 : 0} />
        </label>
        <label>
          Right chain error
          <meter min={0} max={100} high={50} value={error ? error.right * 100 : 0} />
        </label>
        {position &&
          MOTORS.map(([motor, name]) => (
            <label key={motor}>
              {name} motor power
              <meter min={0} max={MOTOR_POWER_MAX} high={240} value={position.power[motor]} />
            </label>
          ))}
      </div>
    </section>
  );
};
