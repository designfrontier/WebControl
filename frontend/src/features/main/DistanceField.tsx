import type { Ref } from "react";
import type { Units } from "../../protocol";
import { unitsLabel } from "./controls";

interface Props {
  id: string;
  label: string;
  unitsName: string;
  text: string;
  onText: (text: string) => void;
  decimals: number;
  units: Units | null;
  onToggleUnits: (units: Units) => void;
  unitsDisabled: boolean;
  inputRef?: Ref<HTMLInputElement>;
  invalid: boolean;
}

/** A jog distance input with its units switch; the switch asks the server, which echoes the new units back. */
export const DistanceField = ({
  id,
  label,
  unitsName,
  text,
  onText,
  decimals,
  units,
  onToggleUnits,
  unitsDisabled,
  inputRef,
  invalid,
}: Props) => (
  <div className="distance-field">
    <label htmlFor={id}>{label}</label>
    <input
      id={id}
      ref={inputRef}
      type="text"
      inputMode="decimal"
      autoComplete="off"
      value={text}
      aria-invalid={invalid}
      aria-describedby={invalid ? `${id}-error` : undefined}
      onChange={(event) => onText(event.target.value)}
    />
    <button
      type="button"
      disabled={unitsDisabled || units === null}
      onClick={units ? () => onToggleUnits(units) : undefined}
      title="Switch units"
    >
      {`${unitsName}: ${units ? unitsLabel(units) : "…"}`}
    </button>
    {invalid && (
      <p id={`${id}-error`} className="field-error">
        Enter a positive number with at most {decimals} decimal places.
      </p>
    )}
  </div>
);
