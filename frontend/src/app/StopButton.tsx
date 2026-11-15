import { useClient, useMachine } from "./context";

/** Stop is always enabled: the server accepts stopRun in every state, and it clears alarms. */
export const StopButton = ({ label = "Stop" }: { label?: string }) => {
  const client = useClient();
  const alarm = useMachine((s) => s.alarm);
  return (
    <button
      type="button"
      className="danger stop-button"
      data-alarm={alarm !== null}
      onClick={() => client.sendAction("stopRun")}
    >
      {label}
    </button>
  );
};
