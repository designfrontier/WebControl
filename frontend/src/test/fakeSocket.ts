import { act } from "@testing-library/react";
import type { ActionCommand, ClientEvent, ClientEvents } from "../protocol";
import type { SocketLike } from "../socket/client";

type Listener = (...args: unknown[]) => void;

export interface Emitted {
  event: string;
  payload: unknown;
}

export interface FakeSocket extends SocketLike {
  /** Everything the client emitted, oldest first. */
  readonly emitted: Emitted[];
  /** Simulates the server accepting the connection, giving this socket `id`. */
  serverConnect(id?: string): void;
  serverDisconnect(): void;
  /** Pushes one `message` envelope the way UIProcessor does: data as JSON text with dataFormat "json". */
  serverMessage(command: string, data: unknown): void;
  /** Pushes a raw `message` payload, for envelopes that are not JSON-encoded or are malformed. */
  serverRaw(payload: unknown): void;
  emittedEvents<E extends ClientEvent>(event: E): ClientEvents[E][];
  /** The processAction calls the client made, as [command, arg, arg1], minus the viewer's statusRequest on connect. */
  actions(): [ActionCommand, string | number, string | number][];
  clearEmitted(): void;
}

export const createFakeSocket = ({ connected = false, id = "client-1" } = {}): FakeSocket => {
  const listeners = new Map<string, Set<Listener>>();
  const emitted: Emitted[] = [];
  let state = { connected, id: connected ? id : undefined };

  const fire = (event: string, ...args: unknown[]) =>
    act(() => {
      listeners.get(event)?.forEach((listener) => listener(...args));
    });

  return {
    get id() {
      return state.id;
    },
    get connected() {
      return state.connected;
    },
    emitted,
    on(event, listener) {
      const set = listeners.get(event) ?? new Set<Listener>();
      set.add(listener as Listener);
      listeners.set(event, set);
      return this;
    },
    emit(event, ...args) {
      emitted.push({ event, payload: args[0] });
      return this;
    },
    connect() {
      return this;
    },
    disconnect() {
      state = { connected: false, id: undefined };
      return this;
    },
    serverConnect(newId = id) {
      state = { connected: true, id: newId };
      fire("connect");
    },
    serverDisconnect() {
      state = { connected: false, id: undefined };
      fire("disconnect", "transport close");
    },
    serverMessage(command, data) {
      fire("message", { command, data: JSON.stringify(data), dataFormat: "json" });
    },
    serverRaw(payload) {
      fire("message", payload);
    },
    emittedEvents: <E extends ClientEvent>(event: E) =>
      emitted.filter((e) => e.event === event).map((e) => e.payload as ClientEvents[E]),
    actions: () =>
      emitted
        .filter((e) => e.event === "action")
        .map((e): [ActionCommand, string | number, string | number] => {
          const { command, arg, arg1 } = (e.payload as ClientEvents["action"]).data;
          return [command as ActionCommand, arg, arg1];
        })
        .filter(([command]) => command !== "statusRequest"),
    clearEmitted: () => {
      emitted.length = 0;
    },
  };
};
