import { useCallback, useEffect, useState } from "react";
import type { Fetch } from "../../api/http";
import { useFetch } from "../../app/context";

export type Load<T> =
  | { status: "loading"; reload: () => void }
  | { status: "ready"; data: T; reload: () => void }
  | { status: "error"; error: unknown; reload: () => void };

type Settled<T> = { status: "ready"; data: T } | { status: "error"; error: unknown };

/** Runs `load` with the app's fetch whenever `key` changes; keep `load` stable (module-level or memoised). */
export const useLoad = <T>(load: (fetchImpl: Fetch, key: string) => Promise<T>, key = ""): Load<T> => {
  const fetchImpl = useFetch();
  const [generation, setGeneration] = useState(0);
  const [settled, setSettled] = useState<{ token: string; result: Settled<T> } | null>(null);
  const token = `${generation}:${key}`;

  useEffect(() => {
    let live = true;
    load(fetchImpl, key).then(
      (data) => live && setSettled({ token, result: { status: "ready", data } }),
      (error: unknown) => live && setSettled({ token, result: { status: "error", error } }),
    );
    return () => {
      live = false;
    };
  }, [fetchImpl, load, key, token]);

  const reload = useCallback(() => setGeneration((n) => n + 1), []);
  return settled?.token === token ? { ...settled.result, reload } : { status: "loading", reload };
};

export const errorText = (error: unknown) => (error instanceof Error ? error.message : String(error));
