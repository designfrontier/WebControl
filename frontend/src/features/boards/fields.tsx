import { useId } from "react";
import type { BoardUnits } from "../../protocol";

interface Props {
  label: string;
  value: string;
  onChange: (value: string) => void;
  /** Set for measurements: the field takes decimals and says which unit. */
  units?: BoardUnits;
  error?: string;
  className?: string;
}

export const TextField = ({ label, value, onChange, units, error, className }: Props) => {
  const id = useId();
  const hint = error ?? (units ? `In ${units}` : "");
  return (
    <div className={className ? `field ${className}` : "field"}>
      <label htmlFor={id}>{label}</label>
      <input
        id={id}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        inputMode={units ? "decimal" : undefined}
        autoComplete="off"
        aria-invalid={error ? true : undefined}
        aria-describedby={hint ? `${id}-hint` : undefined}
      />
      {hint && (
        <span id={`${id}-hint`} className={error ? "form-error" : "muted"}>
          {hint}
        </span>
      )}
    </div>
  );
};
