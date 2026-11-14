import { useClient, useMachine } from "../../app/context";
import type { ControlAvailability } from "./controls";
import { DistanceField } from "./DistanceField";
import { useDistanceControl } from "./hooks";

const JOG_DECIMALS = 3;

type Direction = "upLeft" | "up" | "upRight" | "left" | "right" | "downLeft" | "down" | "downRight";

const PAD: ({ direction: Direction; label: string; arrow: string } | "home")[] = [
  { direction: "upLeft", label: "Up left", arrow: "↖" },
  { direction: "up", label: "Up", arrow: "↑" },
  { direction: "upRight", label: "Up right", arrow: "↗" },
  { direction: "left", label: "Left", arrow: "←" },
  "home",
  { direction: "right", label: "Right", arrow: "→" },
  { direction: "downLeft", label: "Down left", arrow: "↙" },
  { direction: "down", label: "Down", arrow: "↓" },
  { direction: "downRight", label: "Down right", arrow: "↘" },
];

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

  return (
    <section className="panel" aria-labelledby="jog-title">
      <h2 id="jog-title">Move sled</h2>
      <div className="jog-pad" role="group" aria-label="Jog">
        {PAD.map((cell) =>
          cell === "home" ? (
            <button
              key="home"
              type="button"
              disabled={!available.manual}
              onClick={() => client.sendAction("home")}
              title="Move to the home position"
            >
              <span aria-hidden="true">⌂</span> Home
            </button>
          ) : (
            <button key={cell.direction} type="button" disabled={!available.manual} onClick={() => jog(cell.direction)}>
              <span aria-hidden="true">{cell.arrow}</span> {cell.label}
            </button>
          ),
        )}
      </div>
      <DistanceField
        id="jog-distance"
        label="Jog distance"
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
      <div className="button-row">
        <button
          type="button"
          disabled={!available.defineHome}
          onClick={() => client.sendAction("defineHome")}
          title="Make the sled's current position the home position"
        >
          Set Home Here
        </button>
        <button type="button" disabled={!available.manual} onClick={() => client.sendAction("returnToCenter")}>
          Return to Center
        </button>
      </div>
      {macros.length > 0 && (
        <div className="button-row" role="group" aria-label="Macros">
          {macros.map(({ command, title }) => (
            <button key={command} type="button" disabled={!available.manual} onClick={() => client.sendAction(command)}>
              {title.trim() || (command === "macro1" ? "Macro 1" : "Macro 2")}
            </button>
          ))}
        </div>
      )}
    </section>
  );
};
