import { useCallback, useEffect, useState } from "react";
import { useFetch } from "../../app/context";
import type { SettingsSection, SettingsSectionId } from "../../protocol";
import { getSettingsSection } from "./api";

export type SectionLoad =
  { status: "loading" } | { status: "error"; message: string } | { status: "ready"; section: SettingsSection };

/** Loads one settings section; `replace` swaps in the section a save returned. */
export const useSettingsSection = (id: SettingsSectionId) => {
  const fetchImpl = useFetch();
  const [load, setLoad] = useState<SectionLoad>({ status: "loading" });
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let current = true;
    getSettingsSection(fetchImpl, id).then(
      (section) => current && setLoad({ status: "ready", section }),
      (error: Error) => current && setLoad({ status: "error", message: error.message }),
    );
    return () => {
      current = false;
    };
  }, [fetchImpl, id, attempt]);

  const retry = useCallback(() => {
    setLoad({ status: "loading" });
    setAttempt((n) => n + 1);
  }, []);
  const replace = useCallback((section: SettingsSection) => setLoad({ status: "ready", section }), []);

  return { load, retry, replace };
};
