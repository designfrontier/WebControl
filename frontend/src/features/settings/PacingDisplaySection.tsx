import { useState } from "react";
import { useFetch, useMachine, useMachineStore } from "../../app/context";
import { savePacingShown } from "./api";

/** Whether every page's run panel shows the line pacing; the pacing itself stays in WebControl Settings. */
export const PacingDisplaySection = () => {
  const fetch = useFetch();
  const store = useMachineStore();
  const stored = useMachine((s) => s.pacing?.showStats ?? false);
  const [saving, setSaving] = useState<boolean | null>(null);
  const [failed, setFailed] = useState(false);

  const choose = (show: boolean) => {
    setFailed(false);
    setSaving(show);
    savePacingShown(fetch, show)
      .then(
        (stats) => store.dispatch({ type: "message", command: "pacingStats", data: stats }),
        () => setFailed(true),
      )
      .finally(() => setSaving(null));
  };

  return (
    <>
      <fieldset className="settings-choice" aria-describedby="pacing-display-note">
        <legend>Line pacing</legend>
        <label>
          <input type="checkbox" checked={saving ?? stored} onChange={(event) => choose(event.target.checked)} />
          Show line pacing in the run panel
        </label>
        <p id="pacing-display-note" className="muted">
          The pacing picker with its lines/min and lost-line stats, on every device. The pacing itself is in WebControl
          Settings.
        </p>
      </fieldset>
      {failed && (
        <p role="alert" className="setting-error">
          Could not save the line pacing display. Try again.
        </p>
      )}
    </>
  );
};
