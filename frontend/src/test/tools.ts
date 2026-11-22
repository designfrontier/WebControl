import type { FileTool, Tool, ToolInput } from "../protocol";
import { fakeFetch, jsonResponse } from "./fakeFetch";

interface Recorded {
  method: string;
  path: string;
  body: unknown;
}

const stored = (input: ToolInput): Tool => ({
  number: input.number,
  description: input.description,
  diameter: input.diameter ?? null,
  units: input.diameter == null ? null : (input.units ?? null),
  flutes: input.flutes ?? null,
  notes: input.notes ?? "",
});

/** A fetch backed by an in-memory tool library that refuses taken and missing numbers as the server does. */
export const toolServer = (
  initial: Tool[],
  fileTools: FileTool[] = [],
  routes: Parameters<typeof fakeFetch>[0] = {},
) => {
  let tools = [...initial];
  const recorded: Recorded[] = [];
  const answer = (next: Tool[], status = 200) => {
    tools = [...next].sort((a, b) => a.number - b.number);
    return jsonResponse({ tools }, status);
  };
  const taken = (number: number) => jsonResponse({ error: `Your tool library already has a tool ${number}.` }, 409);
  const base = fakeFetch({
    "/api/tools": () => jsonResponse({ tools }),
    "/api/files/gcode/tools": () => jsonResponse({ tools: fileTools }),
    ...routes,
  });
  const fetch = async (input: string, init?: RequestInit) => {
    const method = init?.method ?? "GET";
    const number = Number(/^\/api\/tools\/(\d+)$/.exec(input)?.[1]);
    const body = typeof init?.body === "string" ? (JSON.parse(init.body) as ToolInput) : undefined;
    if (method === "GET" || !(input === "/api/tools" || number)) return base(input, init);
    recorded.push({ method, path: input, body });
    if (method === "POST" && body) {
      return tools.some((t) => t.number === body.number) ? taken(body.number) : answer([...tools, stored(body)], 201);
    }
    if (method === "PUT" && body) {
      return body.number !== number && tools.some((t) => t.number === body.number)
        ? taken(body.number)
        : answer([...tools.filter((t) => t.number !== number), stored(body)]);
    }
    return answer(tools.filter((t) => t.number !== number));
  };
  return { fetch, requests: () => recorded, library: () => tools };
};
