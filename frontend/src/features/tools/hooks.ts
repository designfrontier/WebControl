import { useEffect, useReducer, useState } from "react";
import { useFetch, useServerMessage } from "../../app/context";
import type { Tool } from "../../protocol";
import { getToolLibrary } from "./api";

export type LibraryLoad = { status: "loading" } | { status: "ready"; tools: Tool[] } | { status: "error" };

/** The tool library, kept current by every client's edits through the toolLibrary broadcast. */
export const useToolLibrary = (enabled = true) => {
  const fetchImpl = useFetch();
  const [load, setLoad] = useState<LibraryLoad>({ status: "loading" });
  const [attempt, retry] = useReducer((n: number) => n + 1, 0);
  useEffect(() => {
    if (!enabled) return;
    let live = true;
    getToolLibrary(fetchImpl).then(
      ({ tools }) => live && setLoad({ status: "ready", tools }),
      () => live && setLoad({ status: "error" }),
    );
    return () => {
      live = false;
    };
  }, [fetchImpl, enabled, attempt]);
  useServerMessage("toolLibrary", ({ tools }) => setLoad({ status: "ready", tools }));
  const replace = (tools: Tool[]) => setLoad({ status: "ready", tools });
  return { load, retry, replace };
};
