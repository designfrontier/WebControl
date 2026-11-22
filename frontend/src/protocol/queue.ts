import { z } from "zod";
import type { EventSchemas, MessageSchemas } from "./core";
import { libraryPathField } from "./files";

/** What happens before a queued file starts, judged from the bit the files say the spindle will hold then. */
export const QUEUE_BETWEEN = ["sameTool", "toolChange", "toolChangeSkipped", "noTool", "unknownTool"] as const;

export const queueItemSchema = z.object({
  id: z.number().int(),
  /** The file's path in the gcode library, such as "jobs/leg.nc". */
  file: z.string(),
  /** The first tool its T words name; null when it names none. */
  tool: z.number().int().nullable(),
  /**
   * toolChange: Z rises, then the tool-change prompt waits for Resume. Every other value starts right after Z rises:
   * sameTool, noTool (the file names none), unknownTool (no file or tool change so far says which bit is in) and
   * toolChangeSkipped (the bits differ, but the file's override skips every bit change).
   */
  between: z.enum(QUEUE_BETWEEN),
  /** Queue runs of this file pause for no bit change, between files or in the file; Play outside the queue still does. */
  skipBitChanges: z.boolean(),
});
export type QueueItem = z.infer<typeof queueItemSchema>;

/** GET /api/queue and the jobQueue push: the files to run, in order; the first is loaded or running. */
export const jobQueueSchema = z.object({
  items: z.array(queueItemSchema),
  /** idle: waits for Play on its first file or Start; running: starts each file when the one before it ends. */
  state: z.enum(["idle", "running", "stopped"]),
  /** Why the queue stopped; null unless stopped. */
  reason: z.string().nullable(),
  /** The first file is loaded for a job or between files, so it cannot be moved or removed. */
  headLocked: z.boolean(),
  /** What the queue is doing between files. */
  step: z.enum(["none", "finishing", "loading", "raising", "toolChange"]),
  /** The first file's line index where Stop interrupted it, which Continue may start from; null otherwise. */
  stoppedAt: z.number().int().nullable(),
});
export type JobQueue = z.infer<typeof jobQueueSchema>;

export const queueAddSchema = z.object({ file: libraryPathField, skipBitChanges: z.boolean().optional() });
export const queueOverrideSchema = z.object({ id: z.number().int(), skipBitChanges: z.boolean() });
export const queueMoveSchema = z.object({ id: z.number().int(), to: z.number().int().min(0) });
export const queueRemoveSchema = z.object({ id: z.number().int() });
export const queueEmptySchema = z.object({});
/** start: from line 1 after the raise and tool check; stopped: from `stoppedAt`, as Play mid-file restarts. */
export const queueContinueSchema = z.object({ from: z.enum(["start", "stopped"]).optional() });

/** Server -> client commands for the job queue. Rust only. */
export const queueMessages = {
  /** Broadcast whenever the queue, its state or what happens between its files changes. */
  jobQueue: jobQueueSchema,
} satisfies MessageSchemas;

export const queueActions = [] as const;

export const queueEvents = {} satisfies EventSchemas;
