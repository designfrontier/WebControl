import { useState } from "react";
import { useClient, useMachine, useServerMessage } from "../../app/context";
import { StopButton } from "../../app/StopButton";
import { LoadedFile } from "../files/LoadedFile";
import { QueueSummary } from "../queue";
import type { ControlAvailability } from "./controls";
import { useDraft } from "./hooks";
import { PacingControl } from "./PacingControl";

const STEPS = [
  { command: "moveGcodeZ", delta: -1, label: "Previous Z move", text: "<Z" },
  { command: "moveGcodeIndex", delta: -1, label: "Previous line", text: "<1" },
  { command: "moveGcodeIndex", delta: 1, label: "Next line", text: "1>" },
  { command: "moveGcodeZ", delta: 1, label: "Next Z move", text: "Z>" },
] as const;

/** The camera follows updateCamera broadcasts, which the viewer asks for on connect. */
const CameraToggle = () => {
  const client = useClient();
  const [on, setOn] = useState(false);
  useServerMessage("cameraMessage", (message) => {
    if (message.command === "updateCamera") setOn(message.data === "on");
  });
  return (
    <button type="button" aria-pressed={on} onClick={() => client.sendAction("toggleCamera")}>
      Camera
    </button>
  );
};

interface Props {
  available: ControlAvailability;
  lineIndex: number;
}

export const RunPanel = ({ available, lineIndex }: Props) => {
  const client = useClient();
  const pauseButton = useMachine((s) => s.pauseButton);
  const alarm = useMachine((s) => s.alarm);
  const [lineText, setLineText] = useDraft(String(lineIndex + 1));
  const target = Number(lineText);
  const validTarget = Number.isInteger(target) && target >= 1;

  return (
    <section className="run" aria-label="Run">
      <div className="run-status">
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
        <PacingControl />
      </div>
      <QueueSummary />
      <div className="run-dock">
        <LoadedFile className="run-file" />
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
          <StopButton />
        </div>
      </div>
      <div className="step-buttons" role="group" aria-label="Step through gcode">
        {STEPS.map(({ command, delta, label, text }) => (
          <button
            key={label}
            type="button"
            disabled={!available.step}
            onClick={() => client.sendAction(command, delta)}
            title={label}
          >
            <span aria-hidden="true">{text}</span>
            <span className="visually-hidden">{label}</span>
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
        <label htmlFor="gcode-line">Line #</label>
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
        <CameraToggle />
      </form>
    </section>
  );
};
