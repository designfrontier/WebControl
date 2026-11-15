import { z } from "zod";
import { getJson } from "../api/http";
import type { Fetch } from "../api/http";
import { startupStateSchema } from "../protocol";
import { connectSocketIo } from "./socketio";
import type { ConnectTransport, Transport } from "./types";
import { connectWebSocket } from "./websocket";

export type Backend = "python" | "rust";

const backendSchema = z.object({ backend: startupStateSchema.shape.backend });

/** Which backend serves this page; undefined when /api/state cannot be read. Python predating the field is Python. */
export const probeBackend = async (fetch: Fetch): Promise<Backend | undefined> => {
  try {
    const { backend } = await getJson(fetch, "/state", backendSchema);
    return backend ?? "python";
  } catch {
    return undefined;
  }
};

let chosen: ConnectTransport = connectSocketIo;

/** Opens a connection over the transport `selectTransport` picked (socket.io until then). */
export const connectTransport: ConnectTransport = (namespace, options) => chosen(namespace, options);

/** Picks the wire before any connection opens: plain WebSockets for Rust, socket.io otherwise. */
export const selectTransport = async (fetch: Fetch) => {
  const backend = await probeBackend(fetch);
  chosen = backend === "rust" ? connectWebSocket : connectSocketIo;
  return backend;
};

const FAILURES_BEFORE_PROBE = 3;

/** After three failed connects in a row, reloads the page if another backend now answers /api/state. */
export const watchBackend = (
  transport: Transport,
  current: Backend | undefined,
  { fetch, reload }: { fetch: Fetch; reload: () => void },
) => {
  let failures = 0;
  transport.on("connect", () => {
    failures = 0;
  });
  transport.on("connect_error", () => {
    failures += 1;
    if (failures < FAILURES_BEFORE_PROBE) return;
    failures = 0;
    void probeBackend(fetch).then((now) => {
      if (now !== undefined && now !== (current ?? "python")) reload();
    });
  });
};
