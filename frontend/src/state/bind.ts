import type { StartupState } from "../protocol";
import type { ConnectionStatus, WebControlClient } from "../socket/client";
import { STORE_COMMANDS } from "./machine";
import type { MachineEvent, MachineStore } from "./machine";

/** Feeds the store from the socket, and reloads startup state on every (re)connect. Returns an unbind. */
export const bindMachineStore = (
  store: MachineStore,
  client: WebControlClient,
  loadStartup: () => Promise<StartupState>,
): (() => void) => {
  let bound = true;

  const sync = () => {
    // The server resends the sled position on its next report only after someone asks for units.
    client.requestSetting("Computed Settings", "units");
    loadStartup().then(
      (state) => bound && store.dispatch({ type: "startup", state }),
      (error: unknown) => console.warn("Could not load /api/state", error),
    );
  };

  const onStatus = (status: ConnectionStatus) => {
    store.dispatch({ type: "socket", status });
    if (status === "connected") sync();
  };

  const unsubscribers = [
    ...STORE_COMMANDS.map((command) =>
      client.onMessage(command, (data) => store.dispatch({ type: "message", command, data } as MachineEvent)),
    ),
    client.onStatus(onStatus),
  ];
  onStatus(client.status);

  return () => {
    bound = false;
    unsubscribers.forEach((unsubscribe) => unsubscribe());
  };
};
