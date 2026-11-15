/**
 * One connection to one backend namespace. Besides server events it reports its own lifecycle as the events
 * "connect", "disconnect" and "connect_error", and reconnects by itself until `disconnect()` is called.
 */
export interface Transport {
  /** This connection's id while connected; per-client messages carry it in their `client` field. */
  readonly id: string | undefined;
  readonly connected: boolean;
  on(event: string, handler: (...args: never[]) => void): unknown;
  emit(event: string, payload?: unknown): unknown;
  connect(): unknown;
  disconnect(): unknown;
}

export interface TransportOptions {
  /** Open a connection of its own even if one to the same server is already open. */
  separate?: boolean;
}

export type ConnectTransport = (namespace: string, options?: TransportOptions) => Transport;
