import { useClient, useMachine } from "../../app/context";
import { selectProbing, selectRunState } from "../../state/machine";
import { controlsFor } from "./controls";
import type { ControlAvailability } from "./controls";
import { DistanceField } from "./DistanceField";
import { useDistanceControl, useMainScreenState } from "./hooks";
import "./main.css";

const Z_DECIMALS = 5;

interface Props {
  available: ControlAvailability;
  touchPlate: boolean;
}

/** The classic UI's Z-Axis popup (templates/zaxis.html). */
export const ZAxisPanel = ({ available, touchPlate }: Props) => {
  const client = useClient();
  const unitsZ = useMachine((s) => s.unitsZ);
  const distToMoveZ = useMachine((s) => s.distToMoveZ);
  const field = useDistanceControl({
    source: distToMoveZ,
    decimals: Z_DECIMALS,
    convertDecimals: 2,
    toInches: "toInchesZ",
    toMM: "toMMZ",
  });

  const moveZ = (direction: "raise" | "lower") =>
    field.run((distance) => client.sendAction("moveZ", direction, distance));

  return (
    <section className="z-axis" aria-label="Z axis">
      <DistanceField
        id="z-distance"
        label="Z distance"
        unitsName="Z units"
        text={field.text}
        onText={field.setText}
        decimals={Z_DECIMALS}
        units={unitsZ}
        onToggleUnits={field.toggleUnits}
        unitsDisabled={!available.manual}
        inputRef={field.input}
        invalid={field.invalid}
      />
      <div className="button-row">
        <button type="button" disabled={!available.manual} onClick={() => moveZ("raise")}>
          <span aria-hidden="true">↑</span> Raise
        </button>
        <button type="button" disabled={!available.manual} onClick={() => moveZ("lower")}>
          <span aria-hidden="true">↓</span> Lower
        </button>
        <button type="button" disabled={!available.manual} onClick={() => client.sendAction("defineZ0")}>
          Set Z Zero Here
        </button>
        <button
          type="button"
          disabled={!available.manual || !touchPlate}
          onClick={() => client.sendAction("touchZ")}
          title={touchPlate ? "Probe down to the touch plate" : "Enable the touch plate in Advanced Settings"}
        >
          Touch Plate
        </button>
        <button type="button" className="danger" onClick={() => client.sendAction("stopZ")}>
          Stop Z
        </button>
      </div>
    </section>
  );
};

/** The Z-Axis modal: loads whether the touch plate is enabled, which only GET /api/main reports. */
export const ZAxisModal = () => {
  const loaded = useMainScreenState();
  const available = controlsFor(useMachine(selectRunState), false, useMachine(selectProbing));
  return <ZAxisPanel available={available} touchPlate={loaded?.touchPlate ?? false} />;
};
