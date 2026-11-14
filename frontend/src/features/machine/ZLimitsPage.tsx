import { useClient } from "../../app/context";
import { LoadStatus } from "./LoadStatus";
import { ControllerLog, Screen } from "./Screen";
import { useMachineSetup } from "./useLoad";
import { ZJog } from "./ZJog";

export const ZLimitsPage = () => {
  const client = useClient();
  const [setup, retry] = useMachineSetup();
  const supported = setup.status === "ready" && setup.data.firmwareSupportsZaxisLimit;

  return (
    <Screen title="Z-axis limits">
      <p>Router Z limits help Maslow avoid damage from plunging too deep or retracting too far.</p>
      <LoadStatus load={setup} what="machine setup" onRetry={retry} />
      {setup.status === "ready" && !supported && (
        <p className="notice" data-tone="warn">
          Z-axis limits need controller firmware newer than 1.26. The controller reports version{" "}
          {setup.data.controllerFirmwareVersion || "unknown"}.
        </p>
      )}
      <h3>Step 1: Clear the Z-axis limits</h3>
      <button type="button" disabled={!supported} onClick={() => client.sendAction("clearZ")}>
        Clear Z limits
      </button>
      <h3>Step 2: Move Z to each limit and save it</h3>
      <p>Raise Z to its highest safe point and save the maximum, then lower it to its deepest and save the minimum.</p>
      <ZJog />
      <div className="button-grid">
        <button type="button" disabled={!supported} onClick={() => client.sendAction("setMaxZ")}>
          Save Z max
        </button>
        <button type="button" disabled={!supported} onClick={() => client.sendAction("setMinZ")}>
          Save Z min
        </button>
        <button type="button" disabled={!supported} onClick={() => client.sendAction("getZlimits")}>
          Report Z limits
        </button>
      </div>
      <ControllerLog />
    </Screen>
  );
};
