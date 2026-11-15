import { z } from "zod";
import type { ConnectTransport } from "./types";

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

export const connectWebSocket: ConnectTransport = (namespace) => {
  throw new Error(`The plain WebSocket transport is not implemented yet (namespace ${namespace})`);
};
