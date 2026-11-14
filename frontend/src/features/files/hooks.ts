import { useCallback, useEffect, useState } from "react";
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

/** Runs one request at a time, exposing busy and a failure message (the server's own, when it gives one). */
export const useSubmit = () => {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = async (task: () => Promise<void>, failure: string) => {
    setBusy(true);
    setError(null);
    try {
      await task();
    } catch (caught) {
      setError((caught instanceof FormRouteError && caught.reason) || failure);
    } finally {
      setBusy(false);
    }
  };

  return { busy, error, setError, run };
};
