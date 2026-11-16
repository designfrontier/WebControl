import { useId } from "react";
import type { NumberField, OpticalForm, OpticalKey } from "./opticalForm";

interface FieldProps {
  form: OpticalForm;
  errors: Partial<Record<OpticalKey, string>>;
  onChange: (key: OpticalKey, value: string) => void;
}

export const NumberFields = ({
  fields,
  form,
  errors,
  onChange,
  disabled = false,
}: FieldProps & { fields: NumberField[]; disabled?: boolean }) => {
  const id = useId();
  return (
    <div className="field-grid">
      {fields.map(({ key, label, hint }) => (
        <div className="field" key={key}>
          <label htmlFor={`${id}-${key}`}>{label}</label>
          <input
            id={`${id}-${key}`}
            type="text"
            inputMode="decimal"
            value={form[key]}
            disabled={disabled}
            aria-invalid={errors[key] ? true : undefined}
            aria-describedby={`${id}-${key}-hint`}
            onChange={(event) => onChange(key, event.target.value)}
          />
          <small id={`${id}-${key}-hint`} className={errors[key] ? "field-error" : "muted"}>
            {errors[key] ?? hint}
          </small>
        </div>
      ))}
    </div>
  );
};

export const SelectField = ({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: string;
  options: { value: string; label: string }[];
  onChange: (value: string) => void;
}) => {
  const id = useId();
  return (
    <div className="field">
      <label htmlFor={id}>{label}</label>
      <select id={id} value={value} onChange={(event) => onChange(event.target.value)}>
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </div>
  );
};
