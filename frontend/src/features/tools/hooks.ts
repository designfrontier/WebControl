import { useEffect, useReducer, useState } from "react";
import { useFetch, useMachine, useServerMessage } from "../../app/context";
import type { FileTool, Tool } from "../../protocol";
import { getFileTools, getToolLibrary } from "./api";

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

const NONE: FileTool[] = [];

/** The tools the loaded file names, read again whenever the gcode changes. */
export const useFileTools = () => {
  const fetchImpl = useFetch();
  const file = useMachine((s) => s.gcodeFile);
  const [tools, setTools] = useState<FileTool[]>(NONE);
  const [version, reload] = useReducer((n: number) => n + 1, 0);
  useServerMessage("gcodeUpdateCompressed", reload);
  useEffect(() => {
    if (file === null) return;
    let live = true;
    getFileTools(fetchImpl).then(
      (found) => live && setTools(found.tools),
      () => live && setTools(NONE),
    );
    return () => {
      live = false;
    };
  }, [fetchImpl, file, version]);
  return file === null ? NONE : tools;
};
