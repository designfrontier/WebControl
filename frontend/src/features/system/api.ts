import { ApiError, getJson, sendJson } from "../../api/http";
import type { Fetch } from "../../api/http";
import {
  classicErrorSchema,
  helpPageSchema,
  pidSettingsSchema,
  pidTestResponseSchema,
  restoreResponseSchema,
  systemInfoSchema,
  updatesSchema,
} from "../../protocol";
import type { PidTestRequest, UpdateAction } from "../../protocol";

export const getSystemInfo = (fetchImpl: Fetch) => getJson(fetchImpl, "/system/info", systemInfoSchema);

export const getPidSettings = (fetchImpl: Fetch) => getJson(fetchImpl, "/system/pid", pidSettingsSchema);

export const startPidTest = (fetchImpl: Fetch, request: PidTestRequest) =>
  sendJson(fetchImpl, "POST", "/system/pid-test", request, pidTestResponseSchema);

export const getUpdates = (fetchImpl: Fetch) => getJson(fetchImpl, "/system/updates", updatesSchema);

/** Starts webcontrol-update@update.service or @rollback.service; the progress follows in getUpdates. */
export const startUpdate = (fetchImpl: Fetch, action: UpdateAction) =>
  action === "update"
    ? sendJson(fetchImpl, "POST", "/system/updates/install", {}, updatesSchema)
    : sendJson(fetchImpl, "POST", "/system/updates/rollback", {}, updatesSchema);

export const getHelpPage = (fetchImpl: Fetch, path: string) =>
  getJson(fetchImpl, `/system/help?path=${encodeURIComponent(path)}`, helpPageSchema);

/** Where help pages' images are served from, given the path a doc uses (e.g. "../assets/boards/x.png"). */
export const helpAssetUrl = (src: string) => `/api/system/help/${src.replace(/^(\.\.?\/)+/, "")}`;

/** Uploads a backup zip to the classic UI's restore route, which lives outside /api. */
export const restoreBackup = async (fetchImpl: Fetch, file: File) => {
  const body = new FormData();
  body.append("file", file);
  const response = await fetchImpl("/importRestoreWebControl", { method: "POST", body });
  const answer: unknown = await response.json().catch(() => null);
  const parsed = restoreResponseSchema.safeParse(answer);
  if (!response.ok || !parsed.success) {
    const reason = classicErrorSchema.safeParse(answer).data?.error;
    throw new ApiError(response.status, reason ?? `Restore failed with ${response.status}`, answer);
  }
};

/** Download links for the classic routes; the query defeats caches the way the old buttons did. */
export const downloadUrl = (route: "downloadDiagnostics" | "backupWebControl", now = Date.now()) =>
  `/${route}?rnd=${now}`;
