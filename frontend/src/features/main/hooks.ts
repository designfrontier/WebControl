import { useEffect, useRef, useState } from "react";
import { getJson } from "../../api/http";
import { useClient, useFetch, useMachine, useMachineStore } from "../../app/context";
import { mainScreenStateSchema } from "../../protocol";
import type { MainScreenState, Units } from "../../protocol";
import { convertDistance, parseDistance } from "./controls";

/** Loads GET /api/main on every (re)connect and seeds the run status unless a push overtook the request. */
export const useMainScreenState = (): MainScreenState | null => {
  const fetch = useFetch();
  const store = useMachineStore();
  const socket = useMachine((s) => s.socket);
  const [state, setState] = useState<MainScreenState | null>(null);

  useEffect(() => {
    if (socket !== "connected") return;
    let live = true;
    const statusBefore = store.getState().status;
    getJson(fetch, "/main", mainScreenStateSchema).then(
      (loaded) => {
        if (!live) return;
        setState(loaded);
        if (store.getState().status !== statusBefore) return;
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

interface DistanceOptions {
  /** The server's saved distance (distToMove or distToMoveZ). */
  source: number | null;
  decimals: number;
  convertDecimals: number;
  toInches: string;
  toMM: string;
}

/** A jog distance draft plus the units switch that converts it; shared by the XY and Z panels. */
export const useDistanceControl = ({ source, decimals, convertDecimals, toInches, toMM }: DistanceOptions) => {
  const client = useClient();
  const [text, setText] = useDraft(source === null ? "" : String(source));
  const input = useRef<HTMLInputElement>(null);
  const distance = parseDistance(text, decimals);

  const run = (send: (distance: number) => void) => (distance === null ? input.current?.focus() : send(distance));

  const toggleUnits = (current: Units) =>
    client.updateSetting(
      current === "MM" ? toInches : toMM,
      convertDistance(distance ?? source ?? 0, current, convertDecimals),
    );

  return { text, setText, input, distance, run, toggleUnits, invalid: distance === null && text !== "" };
};
