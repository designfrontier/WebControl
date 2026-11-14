import { z } from "zod";
import type { EventSchemas, MessageSchemas } from "./core";

/** Server -> client commands for updates, logs, help and shutdown. */
export const systemMessages = {
  /** "on" when a newer PyInstaller release is available. */
  pyinstallUpdate: z.string(),
} satisfies MessageSchemas;

export const systemActions = ["shutdown", "TurnOffRPI", "update", "updatePyInstaller", "clearLogs"] as const;

export const systemEvents = {} satisfies EventSchemas;
