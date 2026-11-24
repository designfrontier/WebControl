import { getJson } from "../../api/http";
import type { Fetch } from "../../api/http";
import { lastRunSchema, viewerSnapshotSchema } from "../../protocol";

export const getViewerSnapshot = (fetchImpl: Fetch) => getJson(fetchImpl, "/viewer", viewerSnapshotSchema);

export const getLastRun = (fetchImpl: Fetch) => getJson(fetchImpl, "/viewer/last-run", lastRunSchema);
