import { useCloseModal } from "../../app/modalHistory";
import { useClient } from "../../app/context";
import { LoadStatus } from "./LoadStatus";
import { Screen } from "./Screen";
import { SprocketJog } from "./SprocketJog";
import { useMachineSetup } from "./useLoad";

export const ResetChainsPage = () => {
  const client = useClient();
  const close = useCloseModal();
  const [setup, retry] = useMachineSetup();
  const ready = setup.status === "ready" ? setup.data : null;

  return (
    <Screen title="Reset chains">
      <p>
        Use this when the controller has lost track of the chain lengths. Turn each sprocket until a tooth points
        straight up, then put the link you marked during setup back on that top tooth.
      </p>
      <LoadStatus load={setup} what="machine setup" onRetry={retry} />
      <SprocketJog fourMotor={ready?.fourMotor ?? false} />
      <button type="button" className="danger wide" onClick={() => client.sendAction("stopRun")}>
        Stop
      </button>
      <p>
        Confirming tells the controller both chains are extended to
        {ready ? ` ${ready.chainExtendLength} mm` : " the extend chain distance"}.
      </p>
      <button
        type="button"
        className="primary wide"
        onClick={() => {
          client.sendAction("setSprocketsDefault");
          close();
        }}
      >
        I have reset both chains with the marked link on the vertical tooth
      </button>
    </Screen>
  );
};
