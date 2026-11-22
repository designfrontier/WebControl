import { getJson, sendJson } from "../../api/http";
import type { Fetch } from "../../api/http";
import { jobQueueSchema } from "../../protocol";

export const getQueue = (fetchImpl: Fetch) => getJson(fetchImpl, "/queue", jobQueueSchema);

export const addToQueue = (fetchImpl: Fetch, file: string, skipBitChanges: boolean) =>
  sendJson(fetchImpl, "POST", "/queue/add", { file, skipBitChanges }, jobQueueSchema);

export const overrideInQueue = (fetchImpl: Fetch, id: number, skipBitChanges: boolean) =>
  sendJson(fetchImpl, "POST", "/queue/override", { id, skipBitChanges }, jobQueueSchema);

export const moveInQueue = (fetchImpl: Fetch, id: number, to: number) =>
  sendJson(fetchImpl, "POST", "/queue/move", { id, to }, jobQueueSchema);

export const removeFromQueue = (fetchImpl: Fetch, id: number) =>
  sendJson(fetchImpl, "POST", "/queue/remove", { id }, jobQueueSchema);

export const clearQueue = (fetchImpl: Fetch) => sendJson(fetchImpl, "POST", "/queue/clear", {}, jobQueueSchema);

export const continueQueue = (fetchImpl: Fetch, from: "start" | "stopped") =>
  sendJson(fetchImpl, "POST", "/queue/continue", { from }, jobQueueSchema);
