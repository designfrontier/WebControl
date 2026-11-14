import { useEffect, useState } from "react";
import type { Fetch } from "../../api/http";
import { useFetch, useMachine, useServerMessage } from "../../app/context";
import { getMachineSetup } from "./api";

export type Load<T> = { status: "loading" } | { status: "ready"; data: T } | { status: "error"; error: Error };

const asError = (error: unknown) => (error instanceof Error ? error : new Error(String(error)));

/** Runs `load` on mount and whenever `refreshKey` changes; `load` must be stable (a module-level function). */
export const useLoad = <T>(load: (fetchImpl: Fetch) => Promise<T>, refreshKey: unknown = null) => {
  const fetchImpl = useFetch();
  const [state, setState] = useState<Load<T>>({ status: "loading" });
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let live = true;
    load(fetchImpl).then(
      (data) => live && setState({ status: "ready", data }),
      (error: unknown) => live && setState({ status: "error", error: asError(error) }),
    );
    return () => {
      live = false;
    };
  }, [fetchImpl, load, attempt, refreshKey]);

  const retry = () => {
    setState({ status: "loading" });
    setAttempt((n) => n + 1);
  };
  return [state, retry] as const;
};

/**
 * Machine setup facts, refetched on connect and again when the controller reports its firmware version,
 * which the server answers with clearAlarm some seconds after connectionStatus.
 */
export const useMachineSetup = () => {
  const controller = useMachine((s) => s.controller.status);
  const [firmwareReports, setFirmwareReports] = useState(0);
  useServerMessage("clearAlarm", () => setFirmwareReports((n) => n + 1));
  return useLoad(getMachineSetup, `${controller}:${firmwareReports}`);
};
