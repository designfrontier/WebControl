import { z } from "zod";
import { pleaseSchema } from "./core";
import type { EventSchemas, MessageSchemas } from "./core";

/** Server -> client commands that feed the 3D/2D gcode and board viewer. */
export const viewerMessages = {
  /** Byte length of the payload about to follow; the old UI showed a spinner until it arrived. */
  showFPSpinner: z.number(),
  /** base64 zlib-compressed JSON of the parsed gcode moves, or "" when no file is loaded. */
  gcodeUpdateCompressed: z.string(),
  /** base64 zlib-compressed cut-area grid of the current board. */
  boardCutDataUpdateCompressed: z.string(),
  cameraMessage: z.object({ command: z.string(), data: z.unknown() }),
} satisfies MessageSchemas;

export const viewerActions = ["toggleCamera", "queryCamera", "statusRequest"] as const;

/** Both are broadcast: every connected client receives the resent gcode/board. */
export const viewerEvents = {
  checkForGCodeUpdate: pleaseSchema,
  checkForBoardUpdate: pleaseSchema,
} satisfies EventSchemas;
