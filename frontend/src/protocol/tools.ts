import { z } from "zod";
import type { MessageSchemas } from "./core";

export const TOOL_NUMBER_MAX = 99999;
export const TOOL_DESCRIPTION_MAX = 200;
export const TOOL_NOTES_MAX = 1000;
export const TOOL_DIAMETER_MAX = 1000;
export const TOOL_FLUTES_MAX = 20;

export const toolUnitsSchema = z.enum(["mm", "in"]);
export type ToolUnits = z.infer<typeof toolUnitsSchema>;

const toolNumber = z.number().int().min(1).max(TOOL_NUMBER_MAX).describe("The T number the gcode calls it by");

/** One entry in the user's tool library, as the server stores and sends it. */
export const toolSchema = z.object({
  number: toolNumber,
  description: z.string().min(1).max(TOOL_DESCRIPTION_MAX),
  diameter: z.number().positive().max(TOOL_DIAMETER_MAX).nullable(),
  /** Set exactly when `diameter` is. */
  units: toolUnitsSchema.nullable(),
  flutes: z.number().int().min(1).max(TOOL_FLUTES_MAX).nullable(),
  notes: z.string().max(TOOL_NOTES_MAX),
});
export type Tool = z.infer<typeof toolSchema>;

/** POST /api/tools and PUT /api/tools/{number} body; the description must have more than spaces in it. */
export const toolInputSchema = z.object({
  number: toolNumber,
  description: z.string().min(1).max(TOOL_DESCRIPTION_MAX),
  diameter: z.number().positive().max(TOOL_DIAMETER_MAX).nullable().optional(),
  units: toolUnitsSchema.nullable().optional().describe("Required with a diameter"),
  flutes: z.number().int().min(1).max(TOOL_FLUTES_MAX).nullable().optional(),
  notes: z.string().max(TOOL_NOTES_MAX).optional(),
});
export type ToolInput = z.infer<typeof toolInputSchema>;

/** GET /api/tools, every write's answer, and the `toolLibrary` broadcast: sorted by number. */
export const toolLibrarySchema = z.object({ tools: z.array(toolSchema) });
export type ToolLibrary = z.infer<typeof toolLibrarySchema>;

export const toolNumberParamsSchema = z.object({ number: z.string().regex(/^\d+$/).describe("The tool's number") });

export const toolsMessages = {
  /** Broadcast after every change to the tool library. */
  toolLibrary: toolLibrarySchema,
} satisfies MessageSchemas;
