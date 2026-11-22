import { useEffect, useRef, useState } from "react";
import { ApiError } from "../../api/http";
import type { Fetch } from "../../api/http";
import { useFetch, useMachine, useServerMessage } from "../../app/context";
import type { JobQueue, QueueItem, Tool } from "../../protocol";
import { useToolLibrary } from "../tools/hooks";
import { addToQueue, clearQueue, continueQueue, getQueue, moveInQueue, overrideInQueue, removeFromQueue } from "./api";

/** The file name without its folder. */
export const baseName = (file: string) => file.slice(file.lastIndexOf("/") + 1);

/** "Tool 2 (6 mm flat end mill)" with the tool library's description, else "Tool 2". */
export const toolName = (tool: number | null, library: readonly Tool[] = []) => {
  const known = library.find((entry) => entry.number === tool);
  return known ? `Tool ${tool} (${known.description})` : `Tool ${tool}`;
};

const BETWEEN_TEXT: Record<QueueItem["between"], (tool: string) => string> = {
  sameTool: () => "same tool — starts immediately",
  noTool: () => "no tool named — starts immediately",
  unknownTool: (tool) => `${tool}, bit in the spindle unknown — starts immediately`,
  toolChange: (tool) => `${tool} — will pause for a bit change`,
  toolChangeSkipped: (tool) => `${tool} — bit change skipped (override)`,
};

/** What happens before `item` starts, as the queue panel says it. */
export const betweenText = ({ between, tool }: QueueItem, library: readonly Tool[] = []) =>
  BETWEEN_TEXT[between](toolName(tool, library));

/** "Next: shelf.nc · same tool — starts immediately", or null when nothing follows. */
export const nextText = (queue: JobQueue, library: readonly Tool[] = []) => {
  const next = queue.headLocked ? queue.items[1] : queue.items[0];
  const label = queue.headLocked ? "Next" : "First";
  return next ? `${label}: ${baseName(next.file)} · ${betweenText(next, library)}` : null;
};

/** The library's tools once loaded, else none, so names fall back to the bare number. */
export const useToolNames = () => {
  const { load } = useToolLibrary();
  return load.status === "ready" ? load.tools : NO_TOOLS;
};

const NO_TOOLS: Tool[] = [];

const STEP_TEXT = {
  finishing: (file: string) => `Finishing ${file}…`,
  loading: (file: string) => `Loading ${file}…`,
  raising: (file: string) => `Raising Z before ${file}…`,
  toolChange: (file: string) => `Waiting for the bit for ${file}: change it, then press Resume.`,
} as const;

/** One line on where the queue stands. */
export const statusText = (queue: JobQueue) => {
  const head = queue.items[0] ? baseName(queue.items[0].file) : "";
  if (queue.state === "stopped") return `Queue stopped: ${queue.reason ?? "no reason given"}`;
  if (queue.step !== "none") return STEP_TEXT[queue.step](head);
  if (queue.items.length === 0) return "The queue is empty.";
  return queue.state === "running" ? `Running ${head}.` : `Waiting: play ${head} or start the queue.`;
};

export const continueLabel = (queue: JobQueue) => (queue.state === "stopped" ? "Continue queue" : "Start queue");

export const queueApi = (fetchImpl: Fetch) => ({
  add: (file: string, skipBitChanges = false) => addToQueue(fetchImpl, file, skipBitChanges),
  skip: (id: number, skipBitChanges: boolean) => overrideInQueue(fetchImpl, id, skipBitChanges),
  move: (id: number, to: number) => moveInQueue(fetchImpl, id, to),
  remove: (id: number) => removeFromQueue(fetchImpl, id),
  clear: () => clearQueue(fetchImpl),
  resume: (from: "start" | "stopped" = "start") => continueQueue(fetchImpl, from),
});

/** The server's reason, or a generic one. */
export const refusal = (error: unknown, fallback: string) => {
  const body = error instanceof ApiError ? (error.body as { error?: string } | undefined) : undefined;
  return body?.error ?? fallback;
};

/** GET /api/queue on every (re)connect, then the jobQueue pushes. */
export const useJobQueue = () => {
  const fetch = useFetch();
  const socket = useMachine((s) => s.socket);
  const [queue, setQueue] = useState<JobQueue | null>(null);
  const updates = useRef(0);
  useServerMessage("jobQueue", (next) => {
    updates.current += 1;
    setQueue(next);
  });
  /** Runs a write; its answer is dropped when a push since it was sent is newer. */
  const write = async (task: () => Promise<JobQueue>) => {
    const issued = updates.current;
    const next = await task();
    if (updates.current === issued) setQueue(next);
  };
  useEffect(() => {
    if (socket !== "connected") return;
    let live = true;
    const issued = updates.current;
    getQueue(fetch).then(
      (loaded) => {
        // A push since the request is newer than this answer.
        if (live && updates.current === issued) setQueue(loaded);
      },
      // An older backend has no queue; the panel stays hidden.
      () => {},
    );
    return () => {
      live = false;
    };
  }, [socket, fetch]);
  return [queue, write] as const;
};
