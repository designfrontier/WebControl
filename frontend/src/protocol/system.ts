import { z } from "zod";
import type { EventSchemas, MessageSchemas } from "./core";

/** Server -> client commands for updates, logs, help and shutdown. */
export const systemMessages = {
  /** "on" when a newer PyInstaller release is available. */
  pyinstallUpdate: z.string(),
} satisfies MessageSchemas;

export const systemActions = ["shutdown", "TurnOffRPI", "update", "updatePyInstaller", "clearLogs"] as const;

export const systemEvents = {} satisfies EventSchemas;

/** The socket.io namespace LogStreamer pushes log lines on. */
export const LOGS_NAMESPACE = "/MaslowCNCLogs";

/** LogStreamer's `message` payloads: a line of the full or abridged log, or a periodic logger state. */
export const logMessageSchema = z.discriminatedUnion("log", [
  z.object({ log: z.enum(["log", "alog"]), data: z.string(), state: z.boolean() }),
  z.object({ log: z.literal("state"), state: z.boolean() }),
]);
export type LogMessage = z.infer<typeof logMessageSchema>;

/** GET /api/system/info */
export const systemInfoSchema = z.object({
  version: z.string(),
  platform: z.string(),
  controllerFirmwareVersion: z.number(),
  firmware: z.object({ stock: z.string().nullable(), holey: z.string().nullable(), custom: z.string().nullable() }),
  canTurnOff: z.boolean(),
  fourMotor: z.boolean(),
  /** The version narrowed by fingerprint ("51.292" or "51.29x" for a 51.29 report); null if unknown. */
  firmwareBuild: z.string().nullable().default(null),
  /** Only holey 51.292 and later store Z limits. */
  firmwareSupportsZaxisLimit: z.boolean().default(false),
  update: z.object({ available: z.boolean(), version: z.string().nullable() }),
});
export type SystemInfo = z.infer<typeof systemInfoSchema>;

const gainsSchema = z.object({ kp: z.string(), ki: z.string(), kd: z.string() });

/** GET /api/system/pid */
export const pidSettingsSchema = z.object({
  velocity: gainsSchema,
  position: gainsSchema,
  fourMotor: z.boolean(),
});
export type PidSettings = z.infer<typeof pidSettingsSchema>;

export const motorSchema = z.enum(["L", "R", "M", "N"]);
export const pidKindSchema = z.enum(["velocity", "position"]);
export type Motor = z.infer<typeof motorSchema>;
export type PidKind = z.infer<typeof pidKindSchema>;

/** POST /api/system/pid-test body; numbers stay as typed because the backend splices them into B13/B14. */
export const pidTestRequestSchema = z.object({
  kind: pidKindSchema,
  motor: motorSchema,
  start: z.string(),
  stop: z.string(),
  steps: z.string(),
  version: z.string(),
  time: z.string().optional().describe('Required when kind is "position".'),
  kp: z.string(),
  ki: z.string(),
  kd: z.string(),
});
export type PidTestRequest = z.infer<typeof pidTestRequestSchema>;

export const pidTestResponseSchema = z.object({ status: z.literal("sent") });

/** The inner payload of an updatePIDData message (machineMessages types only the outer wrapper). */
export const pidResultSchema = z.object({
  result: z.enum(["velocity", "position"]),
  version: z.coerce.string(),
  data: z.array(z.union([z.number(), z.string()])),
});
export type PidResult = z.infer<typeof pidResultSchema>;

/** GET /api/system/releases */
export const releasesSchema = z.object({
  current: z.string(),
  latest: z.string().nullable(),
  releases: z.array(z.object({ tag: z.string(), title: z.string(), body: z.string() })),
});
export type Releases = z.infer<typeof releasesSchema>;

/** GET /api/system/help?path= : a docs/ page as markdown with Jekyll and raw HTML already stripped. */
export const helpPageSchema = z.object({ path: z.string(), title: z.string(), markdown: z.string() });
export type HelpPage = z.infer<typeof helpPageSchema>;

/** POST /importRestoreWebControl (the classic UI's route) answers with its status code in the body. */
export const restoreResponseSchema = z.object({ status: z.number() });

/** GET /api/system/help query. */
export const helpQuerySchema = z.object({ path: z.string().optional() });
