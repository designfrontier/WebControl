import type { z } from "zod";

export class ApiError extends Error {
  readonly status: number;
  /** The parsed JSON error body, when the server sent one. */
  readonly body: unknown;

  constructor(status: number, message: string, body?: unknown) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.body = body;
  }
}

export type Fetch = (input: string, init?: RequestInit) => Promise<Response>;

/** Calls `/api${path}` and validates the JSON body against `schema`. */
export const requestJson = async <S extends z.ZodType>(
  fetchImpl: Fetch,
  path: string,
  schema: S,
  init: RequestInit = {},
): Promise<z.output<S>> => {
  const method = init.method ?? "GET";
  const response = await fetchImpl(`/api${path}`, {
    ...init,
    headers: {
      Accept: "application/json",
      ...(init.body ? { "Content-Type": "application/json" } : {}),
      ...Object.fromEntries(new Headers(init.headers)),
    },
  });
  if (!response.ok) {
    const body: unknown = await response.json().catch(() => undefined);
    throw new ApiError(response.status, `${method} /api${path} failed with ${response.status}`, body);
  }
  const parsed = schema.safeParse(await response.json());
  if (!parsed.success) throw new ApiError(response.status, `${method} /api${path} returned an unexpected shape`);
  return parsed.data;
};

export const getJson = <S extends z.ZodType>(fetchImpl: Fetch, path: string, schema: S) =>
  requestJson(fetchImpl, path, schema);

export const sendJson = <S extends z.ZodType>(
  fetchImpl: Fetch,
  method: "POST" | "PUT" | "PATCH" | "DELETE",
  path: string,
  body: unknown,
  schema: S,
) => requestJson(fetchImpl, path, schema, { method, body: JSON.stringify(body) });
