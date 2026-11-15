import { useMachine } from "../../app/context";
import { selectGcodeLocked, selectProbing, selectRunState } from "../../state/machine";
import { Viewer } from "../viewer";
import { ControllerConsole } from "./ControllerConsole";
import { controlsFor } from "./controls";
import { useMainScreenState } from "./hooks";
import { JogPanel } from "./JogPanel";
import { Readouts } from "./Readouts";
import { RunPanel } from "./RunPanel";
import "./main.css";

/** The one screen: the viewer takes most of the width and the compact controls sit beside it. */
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
      <section className="control-panel" aria-labelledby="controls-title">
        <h2 id="controls-title">Controls</h2>
        <JogPanel available={available} />
        <RunPanel available={available} lineIndex={lineIndex ?? 0} />
        <Readouts lineIndex={lineIndex} />
        <ControllerConsole />
      </section>
    </div>
  );
};
