import { z } from "zod";
import type { ConnectTransport, Transport } from "./types";

/** One text frame of the plain-WebSocket transport; contract/README.md specifies the protocol. */
export const frameSchema = z.object({
  ns: z.string().describe('The namespace, e.g. "/MaslowCNC"'),
  event: z.string().describe('The socket event name, e.g. "message" or "action"'),
  data: z.unknown().describe("The payload socket.io would carry as the event's single argument"),
});
export type Frame = z.infer<typeof frameSchema>;

export const encodeFrame = (ns: string, event: string, data: unknown): string => JSON.stringify({ ns, event, data });

export const decodeFrame = (text: string): Frame | undefined => {
  try {
    return frameSchema.safeParse(JSON.parse(text)).data;
  } catch {
    return undefined;
  }
};

/** The URL a namespace's WebSocket connects to, relative to the page's own origin. */
export const webSocketUrl = (namespace: string, { protocol, host }: Pick<Location, "protocol" | "host">) =>
  `${protocol === "https:" ? "wss:" : "ws:"}//${host}/ws?ns=${encodeURIComponent(namespace)}`;

/** 1 s doubling to a 5 s cap, each delay randomised by ±50%; `random` is in [0, 1). */
export const reconnectDelay = (attempt: number, random: () => number) =>
  Math.min(1000 * 2 ** attempt, 5000) * (0.5 + random());

/** The part of the browser's WebSocket this transport uses. */
export interface WebSocketLike {
  onmessage: ((event: MessageEvent) => void) | null;
  onclose: ((event: CloseEvent) => void) | null;
  send(data: string): void;
  close(): void;
}

export interface WebSocketDeps {
  open?: (url: string) => WebSocketLike;
  location?: Pick<Location, "protocol" | "host">;
  random?: () => number;
}

type Listener = (...args: never[]) => void;

const sidOf = (data: unknown) => {
  const parsed = z.object({ sid: z.string() }).safeParse(data);
  return parsed.success ? parsed.data.sid : undefined;
};

/**
 * A Transport over one WebSocket per namespace. It reports "connect" only after the server's handshake frame and
 * reconnects by itself until `disconnect()`. Emits while not connected are dropped, so no stale jog runs later.
 */
export const createWebSocketTransport = (namespace: string, deps: WebSocketDeps = {}): Transport => {
  const open = deps.open ?? ((url: string) => new WebSocket(url));
  const random = deps.random ?? Math.random;
  const listeners = new Map<string, Set<Listener>>();
  let socket: WebSocketLike | undefined;
  let id: string | undefined;
  let wanted = true;
  let attempt = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const fire = (event: string, arg?: unknown) =>
    listeners.get(event)?.forEach((listener) => (listener as (arg: unknown) => void)(arg));

  const received = (raw: unknown) => {
    const frame = typeof raw === "string" ? decodeFrame(raw) : undefined;
    if (!frame || frame.ns !== namespace) return;
    if (id !== undefined) return frame.event === "connect" ? undefined : fire(frame.event, frame.data);
    const sid = frame.event === "connect" ? sidOf(frame.data) : undefined;
    if (sid === undefined) return;
    id = sid;
    attempt = 0;
    fire("connect");
  };

  const closed = () => {
    const wasConnected = id !== undefined;
    socket = undefined;
    id = undefined;
    if (wasConnected) fire("disconnect", "transport close");
    else fire("connect_error", new Error(`WebSocket ${namespace} closed before its handshake`));
    if (!wanted) return;
    timer = setTimeout(start, reconnectDelay(attempt, random));
    attempt += 1;
  };

  const start = () => {
    timer = undefined;
    const next = open(webSocketUrl(namespace, deps.location ?? window.location));
    next.onmessage = (event) => received(event.data);
    next.onclose = closed;
    socket = next;
  };

  const transport: Transport = {
    get id() {
      return id;
    },
    get connected() {
      return id !== undefined;
    },
    on(event, handler) {
      listeners.set(event, (listeners.get(event) ?? new Set<Listener>()).add(handler));
      return transport;
    },
    emit(event, payload) {
      if (id !== undefined) socket?.send(encodeFrame(namespace, event, payload));
      return transport;
    },
    connect() {
      wanted = true;
      if (socket) return transport;
      clearTimeout(timer);
      start();
      return transport;
    },
    disconnect() {
      wanted = false;
      clearTimeout(timer);
      timer = undefined;
      if (socket) {
        socket.onmessage = null;
        socket.onclose = null;
        socket.close();
      }
      socket = undefined;
      id = undefined;
      return transport;
    },
  };
  start();
  return transport;
};

/** Every namespace gets a socket of its own. */
export const connectWebSocket: ConnectTransport = (namespace) => createWebSocketTransport(namespace);
