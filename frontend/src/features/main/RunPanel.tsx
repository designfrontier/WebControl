import { useClient, useMachine } from "../../app/context";
import type { MainScreenState } from "../../protocol";
import type { ControlAvailability } from "./controls";
import { useDraft } from "./hooks";

const STEPS = [
  { command: "moveGcodeZ", delta: -1, label: "Previous Z move", arrow: "⇤" },
  { command: "moveGcodeIndex", delta: -1, label: "Previous line", arrow: "←" },
  { command: "moveGcodeIndex", delta: 1, label: "Next line", arrow: "→" },
  { command: "moveGcodeZ", delta: 1, label: "Next Z move", arrow: "⇥" },
] as const;

interface Props {
  available: ControlAvailability;
  /** The gcode line from GET /api/main, used until the server pushes a gcodePositionMessage. */
  loadedGcode: MainScreenState["gcode"] | null;
}

export const RunPanel = ({ available, loadedGcode }: Props) => {
  const client = useClient();
  const pauseButton = useMachine((s) => s.pauseButton);
  const alarm = useMachine((s) => s.alarm);
  const gcodePosition = useMachine((s) => s.gcodePosition);
  const lineIndex = gcodePosition?.lineIndex ?? loadedGcode?.lineIndex ?? 0;
  const line = gcodePosition?.line ?? loadedGcode?.line ?? "";
  const [lineText, setLineText] = useDraft(String(lineIndex + 1));
  const target = Number(lineText);
  const validTarget = Number.isInteger(target) && target >= 1;

  return (
    <section className="panel" aria-labelledby="run-title">
      <h2 id="run-title">Run</h2>
      <div className="run-buttons">
        <button type="button" className="go" disabled={!available.play} onClick={() => client.sendAction("startRun")}>
          Play
        </button>
        <button
          type="button"
          className="caution"
          disabled={!available.pause}
          onClick={() => client.sendAction(pauseButton === "Pause" ? "pauseRun" : "resumeRun")}
        >
          {pauseButton}
        </button>
        <button
          type="button"
          className="danger"
          data-alarm={alarm !== null}
          onClick={() => client.sendAction("stopRun")}
        >
          Stop
        </button>
      </div>
      <div className="alarm-status" role="status" data-alarm={alarm !== null}>
        {alarm === null ? (
          <span>No alarms</span>
        ) : (
          <>
            <span>{alarm}</span>
            <button type="button" onClick={() => client.sendAction("stopRun")}>
              Stop and clear alarm
            </button>
          </>
        )}
      </div>
      <div className="button-row" role="group" aria-label="Step through gcode">
        {STEPS.map(({ command, delta, label, arrow }) => (
          <button
            key={label}
            type="button"
            disabled={!available.step}
            onClick={() => client.sendAction(command, delta)}
          >
            {delta < 0 ? (
              <>
                <span aria-hidden="true">{arrow}</span> {label}
              </>
            ) : (
              <>
                {label} <span aria-hidden="true">{arrow}</span>
              </>
            )}
          </button>
        ))}
      </div>
      <form
        className="goto-line"
        onSubmit={(event) => {
          event.preventDefault();
          if (validTarget) client.sendAction("moveGcodeGoto", target - 1);
        }}
      >
        <label htmlFor="gcode-line">Line number</label>
        <input
          id="gcode-line"
          type="number"
          min={1}
          step={1}
          value={lineText}
          aria-invalid={!validTarget}
          onChange={(event) => setLineText(event.target.value)}
        />
        <button type="submit" disabled={!available.step || !validTarget}>
          Go to line
        </button>
      </form>
      <p className="gcode-line">
        <span className="muted">Line: </span>
        <code>{line || "—"}</code>
      </p>
    </section>
  );
};
