import { z } from "zod";
import type { EventSchemas, MessageSchemas } from "./core";

/** Server -> client commands for the settings screens. */
export const settingsMessages = {
  /** Serial ports found by the "updatePorts" action. */
  updatePorts: z.array(z.string()),
} satisfies MessageSchemas;

export const settingsActions = ["updatePorts"] as const;

/**
 * settingRequest answers with a broadcast `requestedSetting`; updateSetting takes the front-page names
 * (toInches, toMM, toInchesZ, toMMZ, ...) handled by Actions.updateSetting.
 */
export const settingsEvents = {
  settingRequest: z.object({ data: z.object({ section: z.string(), setting: z.string() }) }),
  updateSetting: z.object({ data: z.object({ setting: z.string(), value: z.union([z.string(), z.number()]) }) }),
} satisfies EventSchemas;

/** The old settings modals, keyed by their `/api/settings/<id>` id. */
export const settingsSectionIds = ["maslow", "advanced", "webcontrol", "camera", "gpio"] as const;
export type SettingsSectionId = (typeof settingsSectionIds)[number];

/** One entry of a config section; `type` is string, int, float, bool, options or path in practice. */
export const settingSchema = z.object({
  key: z.string(),
  title: z.string(),
  desc: z.string(),
  type: z.string(),
  value: z
    .union([z.string(), z.number(), z.boolean()])
    .nullable()
    .transform((value) => value ?? ""),
  options: z.array(z.string()).optional(),
  /** Locked unless the controller runs custom firmware (version 100 or later). */
  custom: z.boolean(),
  /** The old form's isnumber check: int/float settings, or string settings whose saved value is a number. */
  numeric: z.boolean(),
});
export type Setting = z.output<typeof settingSchema>;

export const settingsSectionSchema = z.object({
  id: z.enum(settingsSectionIds),
  title: z.string(),
  enableCustom: z.boolean(),
  settings: z.array(settingSchema),
  /** Serial ports from the last "updatePorts" action; only the Maslow section has them. */
  ports: z.array(z.string()).optional(),
});
export type SettingsSection = z.output<typeof settingsSectionSchema>;

/** PUT /api/settings/<id> body: only changed settings; the rest keep their saved values. */
export const settingsUpdateSchema = z.object({ values: z.record(z.string(), z.union([z.string(), z.boolean()])) });
export type SettingsUpdate = z.infer<typeof settingsUpdateSchema>;

/** PUT /api/settings/<id> 400 body: why each rejected setting failed, keyed by setting key. */
export const settingsRejectionSchema = z.object({ errors: z.record(z.string(), z.string()) });
