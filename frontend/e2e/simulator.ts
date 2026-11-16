import { expect } from "@playwright/test";
import type { APIRequestContext } from "@playwright/test";

/** A second `webcontrol-rs`, on `--simulate`, for the specs that need a controller to move. */
export const simulatorPort = Number(process.env.E2E_SIM_PORT ?? Number(process.env.E2E_BACKEND_PORT ?? 5055) + 1);
export const simulatorURL = `http://localhost:${simulatorPort}`;

export const saveSettings = async (
  request: APIRequestContext,
  section: string,
  values: Record<string, string | boolean>,
) => expect(await request.put(`/api/settings/${section}`, { data: { values } })).toBeOK();

/** Every `/MaslowCNCLogs` line from now until `stop()`. */
export const recordLog = (baseURL: string) =>
  new Promise<{ lines: string[]; stop: () => void }>((resolve, reject) => {
    const lines: string[] = [];
    const socket = new WebSocket(`${baseURL.replace(/^http/, "ws")}/ws?ns=${encodeURIComponent("/MaslowCNCLogs")}`);
    socket.addEventListener("message", (message) => {
      const frame = JSON.parse(String(message.data)) as { event: string; data: unknown };
      if (frame.event === "connect") resolve({ lines, stop: () => socket.close() });
      else lines.push(JSON.stringify(frame.data));
    });
    socket.addEventListener("error", () => reject(new Error(`WebSocket to ${baseURL} failed`)));
  });
