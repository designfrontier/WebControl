import { Link } from "react-router";
import { useClient, useMachine } from "../../app/context";
import type { StartupState } from "../../protocol";
import type { ControlAvailability } from "./controls";
import { DistanceField } from "./DistanceField";
import { useDistanceControl } from "./hooks";

const JOG_DECIMALS = 3;

type Direction = "upLeft" | "up" | "upRight" | "left" | "right" | "downLeft" | "down" | "downRight";
type Macro = StartupState["macros"][number]["command"];
type Cell = { direction: Direction; label: string; arrow: string } | "home" | Macro | "zAxis";

/** The classic layout: the jog pad with Macro 1, Macro 2 and Z-Axis down its right-hand side. */
const GRID: Cell[] = [
  { direction: "upLeft", label: "Up left", arrow: "↖" },
  { direction: "up", label: "Up", arrow: "↑" },
  { direction: "upRight", label: "Up right", arrow: "↗" },
  "macro1",
  { direction: "left", label: "Left", arrow: "←" },
  "home",
  { direction: "right", label: "Right", arrow: "→" },
  "macro2",
  { direction: "downLeft", label: "Down left", arrow: "↙" },
  { direction: "down", label: "Down", arrow: "↓" },
  { direction: "downRight", label: "Down right", arrow: "↘" },
  "zAxis",
];

const DEFAULT_MACRO_TITLES: Record<Macro, string> = { macro1: "Macro 1", macro2: "Macro 2" };

export const JogPanel = ({ available }: { available: ControlAvailability }) => {
  const client = useClient();
  const units = useMachine((s) => s.units);
  const distToMove = useMachine((s) => s.distToMove);
  const macros = useMachine((s) => s.macros);
  const field = useDistanceControl({
    source: distToMove,
    decimals: JOG_DECIMALS,
    convertDecimals: JOG_DECIMALS,
    toInches: "toInches",
    toMM: "toMM",
  });

  const jog = (direction: Direction) => field.run((distance) => client.sendAction("move", direction, distance));
  const macroTitle = (command: Macro) =>
    macros.find((macro) => macro.command === command)?.title.trim() || DEFAULT_MACRO_TITLES[command];

  const cell = (entry: Cell) => {
    if (entry === "home")
      return (
        <button
          key="home"
          type="button"
          className="jog-home"
          disabled={!available.manual}
          onClick={() => client.sendAction("home")}
          title="Move to the home position"
        >
          <span aria-hidden="true">⌂ </span>Home
        </button>
      );
    if (entry === "zAxis")
      return (
        <Link key="zAxis" className="button-link side-button" to="/zaxis">
          Z-Axis
        </Link>
      );
    if (entry === "macro1" || entry === "macro2")
      return (
        <button
          key={entry}
          type="button"
          className="side-button"
          disabled={!available.manual}
          onClick={() => client.sendAction(entry)}
        >
          {macroTitle(entry)}
        </button>
      );
    return (
      <button key={entry.direction} type="button" disabled={!available.manual} onClick={() => jog(entry.direction)}>
        <span aria-hidden="true">{entry.arrow}</span>
        <span className="visually-hidden">{entry.label}</span>
      </button>
    );
  };

  return (
    <section className="jog" aria-label="Jog">
      <div className="jog-grid">{GRID.map(cell)}</div>
      <div className="jog-distance">
        <DistanceField
          id="jog-distance"
          label="Distance"
          unitsName="Units"
          text={field.text}
          onText={field.setText}
          decimals={JOG_DECIMALS}
          units={units}
          onToggleUnits={field.toggleUnits}
          unitsDisabled={!available.manual}
          inputRef={field.input}
          invalid={field.invalid}
        />
        <button
          type="button"
          className="side-button set-home"
          disabled={!available.defineHome}
          onClick={() => client.sendAction("defineHome")}
          title="Make the sled's current position the home position"
        >
          Set Home Here
        </button>
      </div>
    </section>
  );
};
