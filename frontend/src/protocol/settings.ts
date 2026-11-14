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
