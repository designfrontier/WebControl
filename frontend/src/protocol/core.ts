import { z } from "zod";

/** The socket.io namespace every WebControl UI event travels on. */
export const NAMESPACE = "/MaslowCNC";

/** Every server push arrives as a `message` event wrapping one command; `data` is JSON text when dataFormat is "json". */
export const envelopeSchema = z.object({
  command: z.string(),
  data: z.unknown(),
  dataFormat: z.string().optional(),
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
