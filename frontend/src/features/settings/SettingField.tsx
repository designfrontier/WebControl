import { useId } from "react";
import type { Setting } from "../../protocol";
import { controlFor, displayDesc, optionLabel, portOptions, selectOptions } from "./form";
import type { FieldValue } from "./form";

interface Props {
  setting: Setting;
  value: FieldValue;
  error?: string;
  locked: boolean;
  /** Why the setting cannot be changed now, beyond the firmware lock. */
  unavailable?: string;
  ports: string[];
  onChange: (key: string, value: FieldValue) => void;
  onRefreshPorts: () => void;
}

/** One setting rendered from its schema: checkbox, select, serial-port picker or text input. */
export const SettingField = ({
  setting,
  value,
  error,
  locked,
  unavailable,
  ports,
  onChange,
  onRefreshPorts,
}: Props) => {
  const id = useId();
  const control = controlFor(setting);
  const text = String(value);
  const describedBy = [`${id}-desc`, (locked || unavailable) && `${id}-locked`, error && `${id}-error`]
    .filter(Boolean)
    .join(" ");
  const shared = {
    id,
    name: setting.key,
    disabled: locked || unavailable !== undefined,
    "aria-describedby": describedBy,
    "aria-invalid": error ? true : undefined,
  };
  const choices = control === "port" ? portOptions(text, ports) : selectOptions(setting, text);

  return (
    <li className="setting" data-locked={locked || undefined}>
      <div className="setting-text">
        <label htmlFor={id} className="setting-title">
          {setting.title}
        </label>
        <p id={`${id}-desc`} className="setting-desc muted">
          {displayDesc(setting.desc)}
        </p>
        {(locked || unavailable) && (
          <p id={`${id}-locked`} className="setting-note">
            {locked ? "Needs custom firmware (version 100 or later)." : unavailable}
          </p>
        )}
      </div>
      <div className="setting-control">
        {control === "checkbox" ? (
          <input
            {...shared}
            type="checkbox"
            checked={value === true}
            onChange={(e) => onChange(setting.key, e.target.checked)}
          />
        ) : control === "text" ? (
          <input
            {...shared}
            type="text"
            inputMode={setting.numeric ? "decimal" : undefined}
            className={setting.type === "path" ? "mono" : undefined}
            value={text}
            onChange={(e) => onChange(setting.key, e.target.value)}
          />
        ) : (
          <div className="setting-select">
            <select {...shared} value={text} onChange={(e) => onChange(setting.key, e.target.value)}>
              {choices.map((option) => (
                <option key={option} value={option}>
                  {optionLabel(option, setting)}
                </option>
              ))}
            </select>
            {control === "port" && (
              <button type="button" onClick={onRefreshPorts}>
                Refresh ports
              </button>
            )}
          </div>
        )}
        {error && (
          <p id={`${id}-error`} className="setting-error">
            {error}
          </p>
        )}
      </div>
    </li>
  );
};
