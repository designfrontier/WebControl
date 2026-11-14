import { useEffect, useState } from "react";
import { getJson } from "../../api/http";
import { useFetch, useMachine, useMachineStore } from "../../app/context";
import { mainScreenStateSchema } from "../../protocol";
import type { MainScreenState } from "../../protocol";

/** Loads GET /api/main on every (re)connect and seeds the store's run status from it. */
export const useMainScreenState = (): MainScreenState | null => {
  const fetch = useFetch();
  const store = useMachineStore();
  const socket = useMachine((s) => s.socket);
  const [state, setState] = useState<MainScreenState | null>(null);

  useEffect(() => {
    if (socket !== "connected") return;
    let live = true;
    getJson(fetch, "/main", mainScreenStateSchema).then(
      (loaded) => {
        if (!live) return;
        setState(loaded);
        store.dispatch({ type: "message", command: "statusMessage", data: loaded.status });
      },
      // The server repeats statusMessage every 5 s, so a failed load only delays enabling the controls.
      () => {},
    );
    return () => {
      live = false;
    };
  }, [socket, fetch, store]);

  return state;
};

/** Editable text that resets whenever the server sends a new `source` value. */
export const useDraft = (source: string): [string, (text: string) => void] => {
  const [draft, setDraft] = useState({ source, text: source });
  if (draft.source !== source) {
    setDraft({ source, text: source });
    return [source, (text) => setDraft({ source, text })];
  }
  return [draft.text, (text) => setDraft({ source, text })];
};
