import { useEffect, useLayoutEffect, useState, useSyncExternalStore } from "react";
import type { ReactNode } from "react";
import { useMachine } from "./context";
import { ThemeContext, applyPreference, forgetPreference, readPreference } from "./themePreference";
import type { Theme } from "./themePreference";

const DARK_QUERY = "(prefers-color-scheme: dark)";

const darkQuery = () => (typeof window.matchMedia === "function" ? window.matchMedia(DARK_QUERY) : null);

const subscribeToSystem = (onChange: () => void) => {
  const query = darkQuery();
  query?.addEventListener("change", onChange);
  return () => query?.removeEventListener("change", onChange);
};

const systemTheme = (): Theme => (darkQuery()?.matches ? "dark" : "light");

/** Shows the server's uiTheme, which every client shares; "system" follows this device. */
export const ThemeProvider = ({ children }: { children: ReactNode }) => {
  const server = useMachine((state) => state.theme);
  const [legacy] = useState(readPreference);
  const preference = server ?? legacy;
  const system = useSyncExternalStore(subscribeToSystem, systemTheme);
  useLayoutEffect(() => applyPreference(preference), [preference]);
  useEffect(() => {
    if (server) forgetPreference();
  }, [server]);

  return (
    <ThemeContext value={{ preference, theme: preference === "system" ? system : preference }}>{children}</ThemeContext>
  );
};
