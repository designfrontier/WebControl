import { useCallback, useEffect, useRef, useState } from "react";
import type { Fetch } from "../../api/http";
import { useFetch } from "../../app/context";
import { FormRouteError } from "./api";

export type Resource<T> = { status: "loading" } | { status: "ready"; data: T } | { status: "error" };

/** Loads `load` on mount and on each `reload()`; `load` must be a stable (module-level) function. */
export const useResource = <T>(load: (fetchImpl: Fetch) => Promise<T>) => {
  const fetchImpl = useFetch();
  const [resource, setResource] = useState<Resource<T>>({ status: "loading" });
  const [version, setVersion] = useState(0);

  useEffect(() => {
    let live = true;
    load(fetchImpl).then(
      (data) => live && setResource({ status: "ready", data }),
      () => live && setResource({ status: "error" }),
    );
    return () => {
      live = false;
    };
  }, [fetchImpl, load, version]);

  const reload = useCallback(() => setVersion((n) => n + 1), []);
  return { resource, reload };
};

export class SubmitTimeout extends Error {
  constructor(message: string) {
    super(message);
    this.name = "SubmitTimeout";
  }
}

const rejectOnAbort = (signal: AbortSignal) =>
  new Promise<never>((_, reject) => signal.addEventListener("abort", () => reject(signal.reason), { once: true }));

const failureMessage = (caught: unknown, failure: string) =>
  caught instanceof SubmitTimeout ? caught.message : (caught instanceof FormRouteError && caught.reason) || failure;

export interface SubmitOptions {
  /** Gives up on the request after this long, with `timeoutMessage` as the error. */
  timeoutMs?: number;
  timeoutMessage?: string;
}

/**
 * Runs one request at a time, exposing busy and a failure message (the server's own, when it gives one).
 * On timeout or unmount it stops waiting and aborts the task's signal, so the task can skip its follow-ups.
 */
export const useSubmit = ({ timeoutMs, timeoutMessage = "The server did not answer in time." }: SubmitOptions = {}) => {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inFlight = useRef<AbortController | null>(null);

  useEffect(() => () => inFlight.current?.abort(), []);

  const run = async (task: (signal: AbortSignal) => Promise<void>, failure: string) => {
    // Not for fetch: a dropped connection cancels the server's load halfway, and Play stays refused.
    const controller = new AbortController();
    inFlight.current = controller;
    const timer =
      timeoutMs === undefined
        ? undefined
        : setTimeout(() => controller.abort(new SubmitTimeout(timeoutMessage)), timeoutMs);
    setBusy(true);
    setError(null);
    try {
      await Promise.race([task(controller.signal), rejectOnAbort(controller.signal)]);
    } catch (caught) {
      setError(failureMessage(caught, failure));
    } finally {
      clearTimeout(timer);
      if (inFlight.current === controller) inFlight.current = null;
      setBusy(false);
    }
  };

  return { busy, error, setError, run };
};

/** A gcode load on a Pi with a large file can take minutes; past this, assume it is stuck. */
export const GCODE_LOAD = {
  timeoutMs: 3 * 60 * 1000,
  timeoutMessage:
    "The server did not finish loading the gcode within 3 minutes. Check the control screen before trying again.",
};
