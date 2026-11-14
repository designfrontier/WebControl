import { useMachine } from "../../app/context";
import { selectRunState } from "../../state/machine";
import { Viewer } from "../viewer";
import { ControllerConsole } from "./ControllerConsole";
import { controlsFor } from "./controls";
import { useMainScreenState } from "./hooks";
import { JogPanel } from "./JogPanel";
import { Readouts } from "./Readouts";
import { RunPanel } from "./RunPanel";
import { ZAxisPanel } from "./ZAxisPanel";
import "./main.css";

export const MainPage = () => {
  const loaded = useMainScreenState();
  const available = controlsFor(useMachine(selectRunState));

  return (
    <div className="main-page">
      <h1>Control</h1>
      <Viewer />
      <div className="control-panels">
        <RunPanel available={available} loadedGcode={loaded?.gcode ?? null} />
        <JogPanel available={available} />
        <ZAxisPanel available={available} touchPlate={loaded?.touchPlate ?? false} />
        <Readouts />
        <ControllerConsole />
      </div>
    </div>
  );
};
