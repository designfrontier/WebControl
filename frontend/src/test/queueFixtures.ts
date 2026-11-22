import type { JobQueue, QueueItem } from "../protocol";

export const queueItem = (over: Partial<QueueItem> = {}): QueueItem => ({
  id: 1,
  file: "jobs/leg.nc",
  tool: 1,
  between: "sameTool",
  ...over,
});

export const jobQueue = (over: Partial<JobQueue> = {}): JobQueue => ({
  items: [],
  state: "idle",
  reason: null,
  headLocked: false,
  step: "none",
  ...over,
});
