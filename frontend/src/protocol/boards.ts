import { z } from "zod";
import type { EventSchemas, MessageSchemas } from "./core";

export const boardInfoSchema = z.object({
  width: z.coerce.number(),
  height: z.coerce.number(),
  thickness: z.coerce.number(),
  centerX: z.coerce.number(),
  centerY: z.coerce.number(),
  boardID: z.string(),
  material: z.string(),
  fileName: z.string(),
});
export type BoardInfo = z.infer<typeof boardInfoSchema>;

/** Server -> client commands for board (workpiece) management. */
export const boardsMessages = {
  boardDataUpdate: boardInfoSchema,
} satisfies MessageSchemas;

export const boardsActions = ["boardProcessGCode", "boardClearBoard"] as const;

export const boardsEvents = {} satisfies EventSchemas;
