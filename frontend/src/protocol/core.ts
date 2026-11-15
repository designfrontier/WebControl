import { z } from "zod";

/** The socket.io namespace every WebControl UI event travels on. */
export const NAMESPACE = "/MaslowCNC";

/** Every server push arrives as a `message` event wrapping one command; `data` is JSON text when dataFormat is "json". */
export const envelopeSchema = z.object({
  command: z.string(),
  data: z.unknown(),
  dataFormat: z
    .string()
    .optional()
    .meta({ examples: ["json", "int", "base64"] }),
});
export type Envelope = z.infer<typeof envelopeSchema>;

export type MessageSchemas = Record<string, z.ZodType>;
export type EventSchemas = Record<string, z.ZodType>;

export const decodeEnvelopeData = (envelope: Envelope): unknown => {
  if (envelope.dataFormat !== "json" || typeof envelope.data !== "string") return envelope.data;
  try {
    return JSON.parse(envelope.data);
  } catch {
    return undefined;
  }
};

/** Config values arrive as numbers or numeric strings depending on which code path sent them. */
export const numeric = z.coerce.number();

export const actionArg = z.union([z.string(), z.number()]);

/** processAction's payload; `command` is narrowed to ActionCommand by the typed client. */
export const actionEventSchema = z.object({
  data: z.object({ command: z.string(), arg: actionArg, arg1: actionArg }),
});

/** The old UI sends this literal for its "please resend" events. */
export const pleaseSchema = z.object({ data: z.literal("Please") });

/** A number, or text Python's float() accepts, which several handlers parse themselves. */
export const numberLike = z.union([z.number(), z.string().regex(/^\s*[-+]?(\d+\.?\d*|\.\d+)([eE][-+]?\d+)?\s*$/)]);

/** The `{ error }` body most /api endpoints answer a 4xx/5xx with. */
export const errorBodySchema = z.object({ error: z.string() });

/** A multipart file field; in the JSON Schema it is a string with format "binary". */
export const binaryField = z.string().meta({ format: "binary" });
