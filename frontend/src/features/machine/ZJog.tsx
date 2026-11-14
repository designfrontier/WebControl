import { useId, useState } from "react";
import { useClient, useMachine } from "../../app/context";
import { selectRunState } from "../../state/machine";
import { parseDistance, unitLabel, unitSwitch } from "./zAxis";

const asText = (value: number | null) => (value === null ? "" : String(value));

/** Z step entry, unit switch, raise/lower and stop: the controls the Z-axis and Z-limit screens share. */
export const ZJog = () => {
  const client = useClient();
  const unitsZ = useMachine((s) => s.unitsZ);
  const stored = useMachine((s) => s.distToMoveZ);
  const running = useMachine((s) => selectRunState(s) === "running");
  const inputId = useId();

  // A unit switch can echo back the same number, so the units are part of what resets the draft.
  const synced = `${unitsZ}:${stored}`;
  const [draft, setDraft] = useState(asText(stored));
  const [seen, setSeen] = useState(synced);
  if (synced !== seen) {
    setSeen(synced);
    setDraft(asText(stored));
  }

  const distance = parseDistance(draft);
  const canMove = distance !== null && !running;
  const move = (direction: "raise" | "lower") => distance !== null && client.sendAction("moveZ", direction, distance);

  const switchUnits = () => {
    if (!unitsZ) return;
    const { setting, value } = unitSwitch(unitsZ, distance ?? stored ?? 0);
    client.updateSetting(setting, value);
  };

  return (
    <div className="z-jog">
      <div className="z-jog-distance">
        <label htmlFor={inputId}>Step</label>
        <input
          id={inputId}
          type="text"
          inputMode="decimal"
          value={draft}
          aria-invalid={distance === null}
          aria-describedby={`${inputId}-units`}
          onChange={(event) => setDraft(event.target.value)}
        />
        <button
          type="button"
          id={`${inputId}-units`}
          disabled={!unitsZ}
          aria-label={unitsZ ? `Units: ${unitLabel(unitsZ)}, switch` : "Units loading"}
          onClick={switchUnits}
        >
          {unitsZ ? unitLabel(unitsZ) : "…"}
        </button>
      </div>
      {distance === null && <p className="field-error">Enter a step greater than zero.</p>}
      {running && <p className="muted">Z moves are disabled while G-code is running.</p>}
      <div className="button-grid">
        <button type="button" disabled={!canMove} onClick={() => move("raise")}>
          ↑ Raise
        </button>
        <button type="button" disabled={!canMove} onClick={() => move("lower")}>
          ↓ Lower
        </button>
        <button type="button" className="danger" onClick={() => client.sendAction("stopZ")}>
          Stop Z
        </button>
      </div>
    </div>
  );
};
