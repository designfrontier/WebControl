import { z } from "zod";
import { pleaseSchema } from "./core";
import type { EventSchemas, MessageSchemas } from "./core";
import { boardInfoSchema } from "./boards";

/**
 * gzip-compressed JSON. The socket labels it "base64" but python-socketio sends the bytes as a binary attachment
 * (an ArrayBuffer in the browser); /api/viewer sends base64 text. "" or null means nothing is loaded.
 */
export const compressedSchema = z.union([z.instanceof(ArrayBuffer), z.instanceof(Uint8Array), z.string(), z.null()]);
export type Compressed = z.infer<typeof compressedSchema>;

/** Server -> client commands that feed the 3D/2D gcode and board viewer. */
export const viewerMessages = {
  /** Byte length of the payload about to follow; the old UI showed a spinner until it arrived. */
  showFPSpinner: z.number(),
  gcodeUpdateCompressed: compressedSchema,
  /** Board cut-area grid: one truthy/falsy cell per square inch, row by row from the bottom left. */
  boardCutDataUpdateCompressed: compressedSchema,
  cameraMessage: z.discriminatedUnion("command", [
    z.object({ command: z.literal("cameraImageUpdated"), data: z.string() }),
    z.object({ command: z.literal("updateCamera"), data: z.enum(["on", "off"]) }),
  ]),
} satisfies MessageSchemas;

export const viewerActions = ["toggleCamera", "queryCamera", "statusRequest"] as const;

/** Both are broadcast: every connected client receives the resent gcode/board. */
export const viewerEvents = {
  checkForGCodeUpdate: pleaseSchema,
  checkForBoardUpdate: pleaseSchema,
} satisfies EventSchemas;

const pointSchema = z.tuple([z.number(), z.number(), z.number()]);

/**
 * One entry of GCodeFile.line3D, in inches relative to home. A "line" is a polyline (dashed for G0 rapids);
 * anything else is a marker at points[0] whose points[1] holds [size, argument, target z].
 */
export const gcodeMoveSchema = z.object({
  type: z.string().nullable(),
  points: z.array(pointSchema),
  dashed: z.boolean(),
  command: z.string().nullable(),
});
export type GcodeMove = z.infer<typeof gcodeMoveSchema>;
export const gcodeMovesSchema = z.array(gcodeMoveSchema);
export const cutCellsSchema = z.array(z.unknown());

const inches = z.number().nullable();

/** GET /api/viewer: what the viewer draws, sent to just this client on (re)connect. */
export const viewerSnapshotSchema = z.object({
  machine: z.object({ bedWidth: inches, bedHeight: inches, motorOffsetY: inches, motorSpacingX: inches }),
  gcode: z.string().nullable(),
  board: boardInfoSchema.nullable(),
  cutData: z.string().nullable(),
});
export type ViewerSnapshot = z.infer<typeof viewerSnapshotSchema>;
