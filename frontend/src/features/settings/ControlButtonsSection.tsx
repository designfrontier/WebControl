import { useState } from "react";
import { useFetch, useMachine, useMachineStore } from "../../app/context";
import type { ControlButton } from "../../protocol";
import { CONTROL_BUTTONS, SLOT_NAMES, choiceLabel, placeButton } from "../main/controlButtons";
import { saveControlButtons } from "./api";

/** The control screen's four button slots, shown at once here and pushed to every other open page. */
export const ControlButtonsSection = () => {
  const fetch = useFetch();
  const store = useMachineStore();
  const buttons = useMachine((s) => s.controlButtons);
  const macros = useMachine((s) => s.macros);
  const [failed, setFailed] = useState(false);

  const show = (shown: ControlButton[]) =>
    store.dispatch({ type: "message", command: "controlButtons", data: { buttons: shown } });
  const choose = (slot: number, button: ControlButton) => {
    const chosen = placeButton(buttons, slot, button);
    setFailed(false);
    show(chosen);
    saveControlButtons(fetch, chosen).catch(() => {
      if (store.getState().controlButtons === chosen) show(buttons);
      setFailed(true);
    });
  };

  return (
    <>
      <fieldset className="settings-choice control-buttons" aria-describedby="control-buttons-note">
        <legend>Control buttons</legend>
        {SLOT_NAMES.map((name, slot) => (
          <label key={name}>
            {name}
            <select value={buttons[slot]} onChange={(event) => choose(slot, event.target.value as ControlButton)}>
              {CONTROL_BUTTONS.map((button) => (
                <option key={button} value={button}>
                  {choiceLabel(button, macros)}
                </option>
              ))}
            </select>
          </label>
        ))}
        <p id="control-buttons-note" className="muted">
          The four buttons beside the jog arrows on every device. Choosing one another slot holds swaps the two. Macros
          take their titles from Settings ▸ Maslow.
        </p>
      </fieldset>
      {failed && (
        <p role="alert" className="setting-error">
          Could not save the control buttons. Try again.
        </p>
      )}
    </>
  );
};
