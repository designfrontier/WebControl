import { Link } from "react-router";
import { useClient, useMachine } from "../../app/context";
import { slotAction } from "./controlButtons";
import type { ControlAvailability } from "./controls";
import { DistanceField } from "./DistanceField";
import { useDistanceControl } from "./hooks";
import { ZMoveButton } from "./ZMoveButton";
import { isZMove } from "./zMoves";

const JOG_DECIMALS = 3;

type Direction = "upLeft" | "up" | "upRight" | "left" | "right" | "downLeft" | "down" | "downRight";
type Cell = { direction: Direction; label: string; arrow: string } | "home" | { slot: number };

/** The classic layout: the jog pad with three button slots down its right-hand side; the fourth is by the distance. */
const GRID: Cell[] = [
  { direction: "upLeft", label: "Up left", arrow: "↖" },
  { direction: "up", label: "Up", arrow: "↑" },
  { direction: "upRight", label: "Up right", arrow: "↗" },
  { slot: 0 },
  { direction: "left", label: "Left", arrow: "←" },
  "home",
  { direction: "right", label: "Right", arrow: "→" },
  { slot: 1 },
  { direction: "downLeft", label: "Down left", arrow: "↙" },
  { direction: "down", label: "Down", arrow: "↓" },
  { direction: "downRight", label: "Down right", arrow: "↘" },
  { slot: 2 },
];
const DISTANCE_SLOT = 3;

/** Beside the Controls heading, so it costs no height; touch screens never show the disabled Park's tooltip. */
export const ParkHint = () => {
  const unset = useMachine((s) => s.park === null && s.controlButtons.includes("park"));
  return unset ? <p className="park-hint muted">Set a park position to use Park.</p> : null;
};

/** One configurable slot (Settings ▸ Display ▸ Control buttons). */
const ControlSlot = ({ slot, available }: { slot: number; available: ControlAvailability }) => {
  const client = useClient();
  const button = useMachine((s) => s.controlButtons[slot]);
  const park = useMachine((s) => s.park);
  const macros = useMachine((s) => s.macros);
  if (button === "zAxis")
    return (
      <Link className="button-link side-button" to="/zaxis">
        Z-Axis
      </Link>
    );
  if (isZMove(button)) return <ZMoveButton move={button} disabled={!available.manual} className="side-button" />;
  const action = slotAction(button, { available, park, macros });
  return (
    <button
      type="button"
      className="side-button"
      disabled={action.disabled}
      title={action.title}
      onClick={() => client.sendAction(action.command)}
    >
      {action.label}
    </button>
  );
};

export const JogPanel = ({ available }: { available: ControlAvailability }) => {
  const client = useClient();
  const units = useMachine((s) => s.units);
  const distToMove = useMachine((s) => s.distToMove);
  const field = useDistanceControl({
    source: distToMove,
    decimals: JOG_DECIMALS,
    convertDecimals: JOG_DECIMALS,
    toInches: "toInches",
    toMM: "toMM",
  });

  const jog = (direction: Direction) => field.run((distance) => client.sendAction("move", direction, distance));

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
    if ("slot" in entry) return <ControlSlot key={`slot-${entry.slot}`} slot={entry.slot} available={available} />;
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
          unitsDisabled={!available.units}
          inputRef={field.input}
          invalid={field.invalid}
        />
        <ControlSlot slot={DISTANCE_SLOT} available={available} />
      </div>
    </section>
  );
};
