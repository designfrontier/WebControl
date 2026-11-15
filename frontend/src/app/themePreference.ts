import { createContext, use } from "react";

export type ThemePreference = "system" | "light" | "dark";
export type Theme = "light" | "dark";

const STORAGE_KEY = "webcontrol.theme";

export const readPreference = (): ThemePreference => {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    return stored === "light" || stored === "dark" ? stored : "system";
  } catch {
    return "system";
  }
};

export const writePreference = (preference: ThemePreference) => {
  try {
    if (preference === "system") localStorage.removeItem(STORAGE_KEY);
    else localStorage.setItem(STORAGE_KEY, preference);
  } catch {
    // Private windows and blocked storage only lose the remembered choice.
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
  cycle: () => void;
}

export const ThemeContext = createContext<ThemeValue>({ preference: "system", theme: "light", cycle: () => {} });

export const useTheme = () => use(ThemeContext);
