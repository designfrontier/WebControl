import { ApiError, getJson, sendJson } from "../../api/http";
import type { Fetch } from "../../api/http";
import {
  helpPageSchema,
  pidSettingsSchema,
  pidTestResponseSchema,
  releasesSchema,
  restoreResponseSchema,
  systemInfoSchema,
} from "../../protocol";
import type { PidTestRequest } from "../../protocol";

export const getSystemInfo = (fetchImpl: Fetch) => getJson(fetchImpl, "/system/info", systemInfoSchema);

export const getPidSettings = (fetchImpl: Fetch) => getJson(fetchImpl, "/system/pid", pidSettingsSchema);

export const startPidTest = (fetchImpl: Fetch, request: PidTestRequest) =>
  sendJson(fetchImpl, "POST", "/system/pid-test", request, pidTestResponseSchema);

export const getReleases = (fetchImpl: Fetch) => getJson(fetchImpl, "/system/releases", releasesSchema);

export const getHelpPage = (fetchImpl: Fetch, path: string) =>
  getJson(fetchImpl, `/system/help?path=${encodeURIComponent(path)}`, helpPageSchema);

/** Where help pages' images are served from, given the path a doc uses (e.g. "../assets/boards/x.png"). */
export const helpAssetUrl = (src: string) => `/api/system/help/${src.replace(/^(\.\.?\/)+/, "")}`;

/** Uploads a backup zip to the classic UI's restore route, which lives outside /api. */
export const restoreBackup = async (fetchImpl: Fetch, file: File) => {
  const body = new FormData();
  body.append("file", file);
  const response = await fetchImpl("/importRestoreWebControl", { method: "POST", body });
  const parsed = restoreResponseSchema.safeParse(await response.json().catch(() => null));
  if (!response.ok || !parsed.success) {
    throw new ApiError(response.status, `Restore failed with ${response.status}`);
  }
};

/** Download links for the classic routes; the query defeats caches the way the old buttons did. */
export const downloadUrl = (route: "downloadDiagnostics" | "backupWebControl", now = Date.now()) =>
  `/${route}?rnd=${now}`;
