import { useState } from "react";
import { useServerMessage } from "../../app/context";
import { getSystemInfo } from "./api";
import { useLoad } from "./useLoad";

/** True when the server reports a newer release, on load or by a later pyinstallUpdate push. */
export const useUpdateAvailable = () => {
  const info = useLoad(getSystemInfo);
  const [pushed, setPushed] = useState<boolean | null>(null);
  useServerMessage("pyinstallUpdate", (value) => setPushed(value === "on"));
  return pushed ?? (info.status === "ready" && info.data.update.available);
};
