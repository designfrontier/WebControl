import { vi } from "vitest";
import type { StartupState } from "../protocol";

type Responder = (init: RequestInit | undefined) => Response | Promise<Response>;
type Route = Responder | object | string | number | boolean | null;

export const jsonResponse = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });

export const startupState = (over: Partial<StartupState> = {}): StartupState => ({
  units: "MM",
  distToMove: 10,
  unitsZ: "MM",
  distToMoveZ: 1,
  homePosition: { x: 0, y: 0 },
  pauseButtonSetting: "Pause",
  hostAddress: "192.168.1.20:5000",
  controllerFirmwareVersion: 0,
  macros: [
    { command: "macro1", title: "Macro 1" },
    { command: "macro2", title: "Macro 2" },
  ],
  ...over,
});

/**
 * A fetch keyed by "METHOD /api/path" (or just "/api/path" for GET). Plain values are returned as JSON;
 * functions build the Response. Unrouted requests get a 404. GET /api/state answers with startupState() by default.
 */
export const fakeFetch = (routes: Record<string, Route> = {}) => {
  const table: Record<string, Route> = { "GET /api/state": startupState(), ...routes };
  return vi.fn(async (input: string, init?: RequestInit) => {
    const method = init?.method ?? "GET";
    const key = `${method} ${input}` in table ? `${method} ${input}` : method === "GET" ? input : "";
    if (!(key in table)) return jsonResponse({ error: "not found" }, 404);
    const route = table[key];
    return typeof route === "function" ? route(init) : jsonResponse(route);
  });
};
