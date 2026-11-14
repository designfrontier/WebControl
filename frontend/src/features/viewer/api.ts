import { getJson } from "../../api/http";
import type { Fetch } from "../../api/http";
import { viewerSnapshotSchema } from "../../protocol";

export const getViewerSnapshot = (fetchImpl: Fetch) => getJson(fetchImpl, "/viewer", viewerSnapshotSchema);
