import { z } from "zod";
import type { EventSchemas, MessageSchemas } from "./core";
import { libraryPathField } from "./files";

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

export const boardUnitsSchema = z.enum(["mm", "inches"]);
export type BoardUnits = z.infer<typeof boardUnitsSchema>;

/** GET /api/boards/current: dimensions in the machine's units, plus the router position for aligning. */
export const boardDetailsSchema = z.object({
  boardID: z.string(),
  material: z.string(),
  width: z.number(),
  height: z.number(),
  thickness: z.number(),
  centerX: z.number(),
  centerY: z.number(),
  units: boardUnitsSchema,
  fileName: z.string(),
  router: z.object({ x: z.number(), y: z.number() }),
});
export type BoardDetails = z.infer<typeof boardDetailsSchema>;

/** Form bodies (multipart/form-data) of the classic board routes; numbers are posted as typed. */
export const openBoardFormSchema = z.object({ selectedBoard: libraryPathField });
export const saveBoardFormSchema = z.object({ selectedDirectory: z.string(), fileName: z.string() });
export const editBoardFormSchema = z.object({
  boardID: z.string(),
  material: z.string(),
  width: z.string(),
  height: z.string(),
  thickness: z.string(),
  centerX: z.string(),
  centerY: z.string(),
  units: boardUnitsSchema,
});
export const trimBoardFormSchema = z.object({
  trimTop: z.string(),
  trimBottom: z.string(),
  trimLeft: z.string(),
  trimRight: z.string(),
  units: boardUnitsSchema,
});
