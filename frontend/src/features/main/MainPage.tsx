import { useMachine } from "../../app/context";
import { selectGcodeLocked, selectProbing, selectRunState } from "../../state/machine";
import { Viewer } from "../viewer";
import { ControllerConsole } from "./ControllerConsole";
import { controlsFor } from "./controls";
import { useMainScreenState } from "./hooks";
import { JogPanel, ParkHint } from "./JogPanel";
import { Readouts } from "./Readouts";
import { RunPanel } from "./RunPanel";
import "./main.css";

/** The one screen: main.css arranges the viewer, the controls, the readouts and the messages for each window shape. */
export const MainPage = () => {
  const loaded = useMainScreenState();
  const available = controlsFor(useMachine(selectRunState), useMachine(selectGcodeLocked), useMachine(selectProbing));
  const gcodePosition = useMachine((s) => s.gcodePosition);
  const lineIndex = gcodePosition?.lineIndex ?? loaded?.gcode.lineIndex ?? null;
  const line = gcodePosition?.line ?? loaded?.gcode.line ?? "";

  return (
    <div className="control-screen">
      <h1 className="visually-hidden">Control</h1>
      <Viewer currentLine={line} />
      <div className="control-panel">
        <section className="control-jog" aria-labelledby="controls-title">
          <div className="controls-heading">
            <h2 id="controls-title">Controls</h2>
            <ParkHint />
          </div>
          <JogPanel available={available} />
        </section>
        <RunPanel available={available} lineIndex={lineIndex ?? 0} />
        <Readouts lineIndex={lineIndex} />
      </div>
      <ControllerConsole />
    </div>
  );
};
