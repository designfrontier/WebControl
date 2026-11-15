import { connectSocketIo } from "./socketio";
import type { ConnectTransport } from "./types";

export type { ConnectTransport, Transport, TransportOptions } from "./types";

/** The one place that picks the wire: swap in connectWebSocket for a backend that speaks plain WebSockets. */
export const connectTransport: ConnectTransport = connectSocketIo;
