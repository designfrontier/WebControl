import { getJson, sendJson } from "../../api/http";
import type { Fetch } from "../../api/http";
import { controlButtonsUpdateSchema, settingsSectionSchema, themeUpdateSchema } from "../../protocol";
import type { ControlButton, SettingsSectionId, SettingsUpdate, ThemePreference } from "../../protocol";

export const saveTheme = (fetchImpl: Fetch, theme: ThemePreference) =>
  sendJson(fetchImpl, "PUT", "/display/theme", { theme }, themeUpdateSchema);

export const saveControlButtons = (fetchImpl: Fetch, buttons: ControlButton[]) =>
  sendJson(fetchImpl, "PUT", "/display/controls", { buttons }, controlButtonsUpdateSchema);

export const getSettingsSection = (fetchImpl: Fetch, id: SettingsSectionId) =>
  getJson(fetchImpl, `/settings/${id}`, settingsSectionSchema);

export const saveSettingsSection = (fetchImpl: Fetch, id: SettingsSectionId, values: SettingsUpdate["values"]) =>
  sendJson(fetchImpl, "PUT", `/settings/${id}`, { values } satisfies SettingsUpdate, settingsSectionSchema);
