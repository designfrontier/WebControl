import { decodeEnvelopeData, envelopeSchema, serverMessages } from "../protocol";
import type { ActionCommand, ClientEvent, ClientEvents, MessageData, ServerCommand } from "../protocol";
import type { Transport } from "../transport";

export type ConnectionStatus = "connecting" | "connected" | "disconnected";

type Handler = (data: never) => void;

export interface WebControlClient {
  readonly status: ConnectionStatus;
  /** Calls `handler` with each validated `command` message meant for this client; returns an unsubscribe. */
  onMessage<C extends ServerCommand>(command: C, handler: (data: MessageData<C>) => void): () => void;
  onStatus(listener: (status: ConnectionStatus) => void): () => void;
  emit<E extends ClientEvent>(event: E, payload: ClientEvents[E]): void;
  sendAction(command: ActionCommand, arg?: string | number, arg1?: string | number): void;
  requestSetting(section: string, setting: string): void;
  updateSetting(setting: string, value: string | number): void;
  close(): void;
}

/** Messages addressed to another socket carry its sid in `client`; the old UI dropped those too. */
const isForSomeoneElse = (data: unknown, ownId: string | undefined) => {
  if (typeof data !== "object" || data === null || !("client" in data)) return false;
  const { client } = data as { client: unknown };
  return client !== "all" && client !== ownId;
};

export const createClient = (socket: Transport): WebControlClient => {
  const handlers = new Map<string, Set<Handler>>();
  const statusListeners = new Set<(status: ConnectionStatus) => void>();
  let status: ConnectionStatus = socket.connected ? "connected" : "connecting";

  const setStatus = (next: ConnectionStatus) => {
    status = next;
    statusListeners.forEach((listener) => listener(next));
  };

  const dispatch = (raw: unknown) => {
    const envelope = envelopeSchema.safeParse(raw);
    if (!envelope.success) return console.warn("Ignoring malformed socket message", raw);
    const { command } = envelope.data;
    const data = decodeEnvelopeData(envelope.data);
    if (isForSomeoneElse(data, socket.id)) return;
    const listeners = handlers.get(command);
    if (!listeners?.size || !(command in serverMessages)) return;
    const parsed = serverMessages[command as ServerCommand].safeParse(data);
    if (!parsed.success) return console.warn(`Ignoring invalid "${command}" message`, parsed.error.issues);
    listeners.forEach((handler) => (handler as (data: unknown) => void)(parsed.data));
  };

  socket.on("message", (raw: never) => dispatch(raw));
  socket.on("connect", () => setStatus("connected"));
  socket.on("disconnect", () => setStatus("disconnected"));
  socket.on("connect_error", () => setStatus("disconnected"));

  const emit = <E extends ClientEvent>(event: E, payload: ClientEvents[E]) => {
    socket.emit(event, payload);
  };

  return {
    get status() {
      return status;
    },
    onMessage: (command, handler) => {
      const set = handlers.get(command) ?? new Set<Handler>();
      set.add(handler as Handler);
      handlers.set(command, set);
      return () => {
        set.delete(handler as Handler);
      };
    },
    onStatus: (listener) => {
      statusListeners.add(listener);
      return () => {
        statusListeners.delete(listener);
      };
    },
    emit,
    sendAction: (command, arg = "", arg1 = "") => emit("action", { data: { command, arg, arg1 } }),
    requestSetting: (section, setting) => emit("settingRequest", { data: { section, setting } }),
    updateSetting: (setting, value) => emit("updateSetting", { data: { setting, value } }),
    close: () => {
      socket.disconnect();
    },
  };
};
