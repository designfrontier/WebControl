import { createContext, use } from "react";
import type { ThemePreference } from "../protocol";

export type { ThemePreference };
export type Theme = "light" | "dark";

const LEGACY_KEY = "webcontrol.theme";

/** The per-browser choice older builds stored, shown only until the server's uiTheme arrives. */
export const readPreference = (): ThemePreference => {
  try {
    const stored = localStorage.getItem(LEGACY_KEY);
    return stored === "light" || stored === "dark" ? stored : "system";
  } catch {
    return "system";
  }
};

export const forgetPreference = () => {
  try {
    localStorage.removeItem(LEGACY_KEY);
  } catch {
    // Blocked storage has nothing to forget.
  }
};

/** Sets data-theme on <html>; theme.css follows prefers-color-scheme when it is absent. */
export const applyPreference = (preference: ThemePreference) =>
  preference === "system"
    ? document.documentElement.removeAttribute("data-theme")
    : document.documentElement.setAttribute("data-theme", preference);

export interface ThemeValue {
  preference: ThemePreference;
  theme: Theme;
}

export const ThemeContext = createContext<ThemeValue>({ preference: "system", theme: "light" });

export const useTheme = () => use(ThemeContext);
