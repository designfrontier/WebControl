import { ApiError, getJson, requestJson, sendJson } from "../../api/http";
import type { Fetch } from "../../api/http";
import { errorBodySchema, toolLibrarySchema } from "../../protocol";
import type { ToolInput } from "../../protocol";

export const getToolLibrary = (fetchImpl: Fetch) => getJson(fetchImpl, "/tools", toolLibrarySchema);

export const addTool = (fetchImpl: Fetch, tool: ToolInput) =>
  sendJson(fetchImpl, "POST", "/tools", tool, toolLibrarySchema);

export const updateTool = (fetchImpl: Fetch, number: number, tool: ToolInput) =>
  sendJson(fetchImpl, "PUT", `/tools/${number}`, tool, toolLibrarySchema);

export const deleteTool = (fetchImpl: Fetch, number: number) =>
  requestJson(fetchImpl, `/tools/${number}`, toolLibrarySchema, { method: "DELETE" });

/** The server's own words for a refusal, else `fallback`. */
export const refusalText = (caught: unknown, fallback: string) => {
  const body = caught instanceof ApiError ? errorBodySchema.safeParse(caught.body) : null;
  return body?.success ? body.data.error : fallback;
};
