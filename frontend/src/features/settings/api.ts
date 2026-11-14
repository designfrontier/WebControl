import { getJson, sendJson } from "../../api/http";
import type { Fetch } from "../../api/http";
import { settingsSectionSchema } from "../../protocol";
import type { SettingsSectionId, SettingsUpdate } from "../../protocol";

export const getSettingsSection = (fetchImpl: Fetch, id: SettingsSectionId) =>
  getJson(fetchImpl, `/settings/${id}`, settingsSectionSchema);

export const saveSettingsSection = (fetchImpl: Fetch, id: SettingsSectionId, values: SettingsUpdate["values"]) =>
  sendJson(fetchImpl, "PUT", `/settings/${id}`, { values } satisfies SettingsUpdate, settingsSectionSchema);
