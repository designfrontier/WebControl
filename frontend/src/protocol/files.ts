import { z } from "zod";
import type { EventSchemas, MessageSchemas } from "./core";

/** Server -> client commands for opening, uploading, saving and editing gcode. */
export const filesMessages = {
  /** Sent after `createDirectory` with the new directory's name. */
  updateDirectories: z.object({ directory: z.string() }),
} satisfies MessageSchemas;

export const filesActions = ["createDirectory", "clearGCode"] as const;

export const filesEvents = {} satisfies EventSchemas;
