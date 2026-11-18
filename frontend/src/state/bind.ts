import type { StartupState } from "../protocol";
import type { ConnectionStatus, WebControlClient } from "../socket/client";
import { STORE_COMMANDS, pushedFields } from "./machine";
import type { MachineMessage, MachineState, MachineStore } from "./machine";

/** Feeds the store from the socket, and reloads startup state on every (re)connect. Returns an unbind. */
export const bindMachineStore = (
  store: MachineStore,
  client: WebControlClient,
  loadStartup: () => Promise<StartupState>,
): (() => void) => {
  let bound = true;
  let generation = 0;
  // Fields pushed since the pending snapshot was requested; the snapshot may predate them.
  let pushedSince = new Set<keyof MachineState>();

  const sync = () => {
    // The server resends the sled position on its next report only after someone asks for units.
    client.requestSetting("Computed Settings", "units");
    const current = ++generation;
    const keep = new Set<keyof MachineState>();
    pushedSince = keep;
    loadStartup().then(
      (state) => bound && current === generation && store.dispatch({ type: "startup", state, keep }),
      (error: unknown) => console.warn("Could not load /api/state", error),
    );
  };

  const onStatus = (status: ConnectionStatus) => {
    store.dispatch({ type: "socket", status });
    if (status === "connected") sync();
  };

  const onMessage = (event: MachineMessage) => {
    pushedFields(event).forEach((field) => pushedSince.add(field));
    store.dispatch(event);
  };

  const unsubscribers = [
    ...STORE_COMMANDS.map((command) =>
      client.onMessage(command, (data) => onMessage({ type: "message", command, data } as MachineMessage)),
    ),
    client.onStatus(onStatus),
  ];
  onStatus(client.status);

  return () => {
    bound = false;
    unsubscribers.forEach((unsubscribe) => unsubscribe());
  };
};
