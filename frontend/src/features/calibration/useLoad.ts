import { useEffect, useState } from "react";
import type { Fetch } from "../../api/http";
import { useFetch } from "../../app/context";

export type Loaded<T> = { status: "loading" } | { status: "ready"; data: T } | { status: "error" };

/** Runs `load` once per mount; pass a module-level function so it stays the same between renders. */
export const useLoad = <T>(load: (fetchImpl: Fetch) => Promise<T>): Loaded<T> => {
  const fetchImpl = useFetch();
  const [state, setState] = useState<Loaded<T>>({ status: "loading" });

  useEffect(() => {
    let live = true;
    load(fetchImpl).then(
      (data) => live && setState({ status: "ready", data }),
      () => live && setState({ status: "error" }),
    );
    return () => {
      live = false;
    };
  }, [fetchImpl, load]);

  return state;
};
