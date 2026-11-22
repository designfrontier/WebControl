import { getJson, sendJson } from "../../api/http";
import type { Fetch } from "../../api/http";
import { jobQueueSchema } from "../../protocol";

export const getQueue = (fetchImpl: Fetch) => getJson(fetchImpl, "/queue", jobQueueSchema);

export const addToQueue = (fetchImpl: Fetch, file: string) =>
  sendJson(fetchImpl, "POST", "/queue/add", { file }, jobQueueSchema);

export const moveInQueue = (fetchImpl: Fetch, id: number, to: number) =>
  sendJson(fetchImpl, "POST", "/queue/move", { id, to }, jobQueueSchema);

export const removeFromQueue = (fetchImpl: Fetch, id: number) =>
  sendJson(fetchImpl, "POST", "/queue/remove", { id }, jobQueueSchema);

export const clearQueue = (fetchImpl: Fetch) => sendJson(fetchImpl, "POST", "/queue/clear", {}, jobQueueSchema);

export const continueQueue = (fetchImpl: Fetch) => sendJson(fetchImpl, "POST", "/queue/continue", {}, jobQueueSchema);
