import { useEffect, useRef, useState } from "react";
import { ApiError } from "../../api/http";
import type { Fetch } from "../../api/http";
import { useFetch, useMachine, useServerMessage } from "../../app/context";
import type { JobQueue, QueueItem } from "../../protocol";
import { addToQueue, clearQueue, continueQueue, getQueue, moveInQueue, removeFromQueue } from "./api";

/** The file name without its folder. */
export const baseName = (file: string) => file.slice(file.lastIndexOf("/") + 1);

/** What happens before `item` starts, as the queue panel says it. */
export const betweenText = ({ between, tool }: QueueItem) =>
  between === "sameTool"
    ? "same tool, starts immediately"
    : between === "toolChange"
      ? `Tool ${tool}, will pause for a tool change`
      : "names no tool, will pause to check the bit";

/** "next: shelf.nc — same tool, starts immediately", or null when nothing follows. */
export const nextText = (queue: JobQueue) => {
  const next = queue.headLocked ? queue.items[1] : queue.items[0];
  return next ? `${queue.headLocked ? "Next" : "First"}: ${baseName(next.file)} — ${betweenText(next)}` : null;
};

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
  add: (file: string) => addToQueue(fetchImpl, file),
  move: (id: number, to: number) => moveInQueue(fetchImpl, id, to),
  remove: (id: number) => removeFromQueue(fetchImpl, id),
  clear: () => clearQueue(fetchImpl),
  resume: () => continueQueue(fetchImpl),
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
