import { useClient, useMachine } from "../../app/context";
import { selectRunState } from "../../state/machine";
import { LoadStatus } from "./LoadStatus";
import { Screen } from "./Screen";
import { useMachineSetup } from "./useLoad";
import { ZJog } from "./ZJog";

export const ZAxisPage = () => {
  const client = useClient();
  const running = useMachine((s) => selectRunState(s) === "running");
  const [setup, retry] = useMachineSetup();
  const touchPlate = setup.status === "ready" && setup.data.touchPlate;

  return (
    <Screen title="Z-axis">
      <ZJog />
      <div className="button-grid">
        <button type="button" disabled={running} onClick={() => client.sendAction("defineZ0")}>
          Define Z zero
        </button>
        <button type="button" disabled={running || !touchPlate} onClick={() => client.sendAction("touchZ")}>
          Touch off Z
        </button>
      </div>
      {setup.status === "ready" && !touchPlate && <p className="muted">Touch off needs a touch plate.</p>}
      <LoadStatus load={setup} what="machine setup" onRetry={retry} />
    </Screen>
  );
};
