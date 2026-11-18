import { z } from "zod";
import type { EventSchemas, MessageSchemas } from "./core";

/** Server -> client commands for updates, logs, help and shutdown. */
export const systemMessages = {
  /** "on" when a newer PyInstaller release is available. */
  pyinstallUpdate: z.string(),
} satisfies MessageSchemas;

export const systemActions = ["shutdown", "TurnOffRPI", "update", "updatePyInstaller", "clearLogs"] as const;

export const systemEvents = {} satisfies EventSchemas;

/** The namespace the log viewer receives log lines on. */
export const LOGS_NAMESPACE = "/MaslowCNCLogs";

/** LogStreamer's `message` payloads: a line of the full or abridged log, or a periodic logger state. */
export const logMessageSchema = z.discriminatedUnion("log", [
  z.object({ log: z.enum(["log", "alog"]), data: z.string(), state: z.boolean() }),
  z.object({ log: z.literal("state"), state: z.boolean() }),
]);
export type LogMessage = z.infer<typeof logMessageSchema>;

export const upgradeStageSchema = z.enum([
  "idle",
  "checking",
  "ready",
  "blocked",
  "backingUp",
  "flashing",
  "restarting",
  "migrating",
  "selfTest",
  "testing",
  "done",
  "failed",
  "rollingBack",
  "rolledBack",
]);
export type UpgradeStage = z.infer<typeof upgradeStageSchema>;

const chainsSchema = z.object({ left: z.number(), right: z.number() });

/** webcontrol-rs's guided holey upgrade (spec/firmware.md C), driven by upgradeHoleyFirmware's steps. */
export const firmwareUpgradeSchema = z.object({
  target: z.string(),
  /** The pinned image's SHA-256; null when the build carries no image. */
  imageSha256: z.string().nullable(),
  stage: upgradeStageSchema,
  message: z.string(),
  /** The boot banner and B05 version pre-flight found. */
  board: z.string().nullable(),
  firmware: z.string().nullable(),
  chains: chainsSchema.nullable(),
  chainsAfter: chainsSchema.nullable(),
  motors: z.array(z.object({ motor: z.string(), passed: z.array(z.boolean()) })),
  /** Where the $$ dump, settings, chain lengths and flash read-back were saved. */
  backup: z.string().nullable(),
  canRollback: z.boolean(),
  /** The version read off the board before the upgrade, which a rollback restores. */
  rollbackFirmware: z.string().nullable().default(null),
  log: z.array(z.string()),
});
export type FirmwareUpgrade = z.infer<typeof firmwareUpgradeSchema>;

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
  /** Only webcontrol-rs sends it; Python's upgrade buttons flash at once. */
  firmwareUpgrade: firmwareUpgradeSchema.optional(),
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

export const updateStatusSchema = z
  .enum(["upToDate", "updateAvailable", "devBuild", "ahead", "unknown"])
  .describe(
    "devBuild: the installed version is the latest release's but built from another commit; " +
      "ahead: newer than the latest release; unknown: GitHub could not be asked or a version is not semver.",
  );
export type UpdateStatusKind = z.infer<typeof updateStatusSchema>;

export const updateActionSchema = z.enum(["update", "rollback"]);
export type UpdateAction = z.infer<typeof updateActionSchema>;

export const updatePhaseSchema = z
  .enum([
    "starting",
    "checking",
    "downloading",
    "verified",
    "backingUp",
    "installing",
    "restarting",
    "healthCheck",
    "rollingBack",
    "finished",
  ])
  .describe("starting: the server started the unit and the updater has not written yet; verified: checksum OK");
export type UpdatePhase = z.infer<typeof updatePhaseSchema>;

export const updateResultSchema = z
  .enum(["updated", "upToDate", "rolledBack", "rollbackFailed", "failed", "interrupted"])
  .describe(
    "rolledBack after an update: it failed and the backup is running again; after a rollback: it worked. " +
      "failed: refused or stopped with nothing changed; interrupted: the updater stopped without finishing.",
  );
export type UpdateResult = z.infer<typeof updateResultSchema>;

/** The last update or rollback started from the page, as webcontrol-update@.service reports it. */
export const updateProgressSchema = z.object({
  action: updateActionSchema,
  phase: updatePhaseSchema,
  running: z.boolean(),
  from: z.string().nullable().describe("The version installed when it started"),
  to: z.string().nullable().describe("The release, or the backup's version for a rollback"),
  startedAt: z.number().nullable().describe("Milliseconds since the epoch"),
  finishedAt: z.number().nullable(),
  result: updateResultSchema.nullable().describe("null while it runs"),
  log: z.array(z.string()).describe("What the updater printed, oldest first"),
});
export type UpdateProgress = z.infer<typeof updateProgressSchema>;

/** GET /api/system/updates: the installed build, the latest published release and webcontrol-update's backup. */
export const updatesSchema = z.object({
  installed: z.object({ version: z.string(), commit: z.string() }),
  repo: z.string().describe("The GitHub repository releases are read from, owner/name"),
  latest: z
    .object({
      tag: z.string(),
      name: z.string(),
      publishedAt: z.string().nullable(),
      url: z.string(),
      notes: z.string().describe("The release notes as GitHub markdown"),
      commit: z.string().nullable().describe("The commit the tag points at, when GitHub said"),
    })
    .nullable(),
  status: updateStatusSchema,
  error: z.string().nullable().describe("Why GitHub could not be asked"),
  checkedAt: z.number().nullable().describe("When GitHub was last asked, in milliseconds since the epoch"),
  backup: z
    .object({ version: z.string(), date: z.string() })
    .nullable()
    .describe("The install webcontrol-update saved before the last update, which --rollback restores"),
  canUpdate: z.boolean().describe("webcontrol-update@.service is installed, so this page can update and roll back"),
  busy: z.string().nullable().describe("Why an update or rollback would be refused now: a job, probe, scan, ..."),
  progress: updateProgressSchema.nullable(),
});
export type Updates = z.infer<typeof updatesSchema>;

/** GET /api/system/help?path= : a docs/ page as markdown with Jekyll and raw HTML already stripped. */
export const helpPageSchema = z.object({ path: z.string(), title: z.string(), markdown: z.string() });
export type HelpPage = z.infer<typeof helpPageSchema>;

/** POST /importRestoreWebControl (the classic UI's route) answers with its status code in the body. */
export const restoreResponseSchema = z.object({ status: z.number() });

/** GET /api/system/help query. */
export const helpQuerySchema = z.object({ path: z.string().optional() });
