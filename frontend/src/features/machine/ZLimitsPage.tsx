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
      <p>
        The controller stores each limit in mm and holds Z there. Zeroing Z shifts the saved limits by the same amount,
        so they stay at the same place on the mechanism. WebControl refuses a command that would pass one, and pauses a
        job before such a line.
      </p>
      <LoadStatus load={setup} what="machine setup" onRetry={retry} />
      {setup.status === "ready" && !supported && (
        <p className="notice" data-tone="warn">
          Z-axis limits need holey firmware 51.292 or later. The controller reports{" "}
          {setup.data.firmwareBuild ?? "no version yet"}.
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
