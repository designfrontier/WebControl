import { createContext, use, useEffect, useEffectEvent, useSyncExternalStore } from "react";
import type { Fetch } from "../api/http";
import type { MessageData, ServerCommand } from "../protocol";
import type { WebControlClient } from "../socket/client";
import type { MachineState, MachineStore } from "../state/machine";

export const ClientContext = createContext<WebControlClient | null>(null);
export const FetchContext = createContext<Fetch | null>(null);
export const MachineContext = createContext<MachineStore | null>(null);

const required = <T>(value: T | null, name: string): T => {
  if (value === null) throw new Error(`${name} used outside <WebControlProvider>`);
  return value;
};

export const useClient = () => required(use(ClientContext), "useClient");

/** The fetch that feature api modules pass to getJson/sendJson, so tests can swap it. */
export const useFetch = () => required(use(FetchContext), "useFetch");

export const useMachineStore = () => required(use(MachineContext), "useMachineStore");

/** Subscribes to one slice of machine state; select stored values, not freshly built objects. */
export const useMachine = <T>(selector: (state: MachineState) => T): T => {
  const store = useMachineStore();
  return useSyncExternalStore(store.subscribe, () => selector(store.getState()));
};

/** Runs `handler` for each `command` message addressed to this client while the component is mounted. */
export const useServerMessage = <C extends ServerCommand>(command: C, handler: (data: MessageData<C>) => void) => {
  const client = useClient();
  const onMessage = useEffectEvent(handler);
  useEffect(() => client.onMessage(command, (data) => onMessage(data)), [client, command]);
};
