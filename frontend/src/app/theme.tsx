import { useLayoutEffect, useState, useSyncExternalStore } from "react";
import type { ReactNode } from "react";
import { ThemeContext, applyPreference, readPreference, useTheme, writePreference } from "./themePreference";
import type { Theme, ThemePreference } from "./themePreference";

const DARK_QUERY = "(prefers-color-scheme: dark)";
const NEXT: Record<ThemePreference, ThemePreference> = { system: "light", light: "dark", dark: "system" };
const LABELS: Record<ThemePreference, string> = { system: "System", light: "Light", dark: "Dark" };

const darkQuery = () => (typeof window.matchMedia === "function" ? window.matchMedia(DARK_QUERY) : null);

const subscribeToSystem = (onChange: () => void) => {
  const query = darkQuery();
  query?.addEventListener("change", onChange);
  return () => query?.removeEventListener("change", onChange);
};

const systemTheme = (): Theme => (darkQuery()?.matches ? "dark" : "light");

export const ThemeProvider = ({ children }: { children: ReactNode }) => {
  const [preference, setPreference] = useState(readPreference);
  const system = useSyncExternalStore(subscribeToSystem, systemTheme);
  useLayoutEffect(() => applyPreference(preference), [preference]);

  const cycle = () => {
    const next = NEXT[preference];
    writePreference(next);
    setPreference(next);
  };

  return (
    <ThemeContext value={{ preference, theme: preference === "system" ? system : preference, cycle }}>
      {children}
    </ThemeContext>
  );
};

export const ThemeToggle = () => {
  const { preference, cycle } = useTheme();
  return (
    <button type="button" className="theme-toggle" onClick={cycle} title="Switch between system, light and dark">
      <span className="theme-toggle-icon" aria-hidden="true">
        ◐
      </span>
      <span className="theme-toggle-text">Theme: {LABELS[preference]}</span>
    </button>
  );
};
