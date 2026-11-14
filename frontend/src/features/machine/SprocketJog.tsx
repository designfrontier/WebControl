import { useClient } from "../../app/context";
import { DIRECTIONS, SPROCKET_STEPS, directionLabel, motorsFor, sprocketDegrees } from "./sprockets";

interface Props {
  fourMotor: boolean;
  /** Called after each turn is sent, e.g. to re-lock the chain extend buttons. */
  onTurn?: () => void;
}

/** Turns each sprocket by a fixed number of degrees (setSprockets). */
export const SprocketJog = ({ fourMotor, onTurn }: Props) => {
  const client = useClient();
  return (
    <div className="sprocket-jog">
      {motorsFor(fourMotor).map((motor) => (
        <fieldset key={motor.id} className="motor">
          <legend>{motor.label}</legend>
          {DIRECTIONS.map((direction) => (
            <div key={direction} className="motor-row" role="group" aria-label={`${motor.label} ${direction}`}>
              <span className="motor-row-label">
                {direction === "clockwise" ? "↻" : "↺"} {directionLabel(direction)}
              </span>
              {SPROCKET_STEPS.map((step) => (
                <button
                  key={step}
                  type="button"
                  aria-label={`${motor.label} ${directionLabel(direction).toLowerCase()} ${step}°`}
                  onClick={() => {
                    client.sendAction("setSprockets", motor.id, sprocketDegrees(motor, direction, step));
                    onTurn?.();
                  }}
                >
                  {step}°
                </button>
              ))}
            </div>
          ))}
        </fieldset>
      ))}
    </div>
  );
};
