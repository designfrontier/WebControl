import { createContext, use } from "react";
import type { ThemePreference } from "../protocol";

export type { ThemePreference };
export type Theme = "light" | "dark";

const STORAGE_KEY = "webcontrol.theme";

/** The server's uiTheme as this browser last saw it, shown only until /api/state loads. */
export const readPreference = (): ThemePreference => {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    return stored === "light" || stored === "dark" ? stored : "system";
  } catch {
    return "system";
  }
};

export const rememberPreference = (preference: ThemePreference) => {
  try {
    if (preference === "system") localStorage.removeItem(STORAGE_KEY);
    else localStorage.setItem(STORAGE_KEY, preference);
  } catch {
    // Blocked storage only costs the next load its head start.
  }
};

/** Sets data-theme on <html>; theme.css follows prefers-color-scheme when it is absent. */
export const applyPreference = (preference: ThemePreference) =>
  preference === "system"
    ? document.documentElement.removeAttribute("data-theme")
    : document.documentElement.setAttribute("data-theme", preference);

/** Each theme's --bg, for the browser chrome around the page. */
const THEME_COLORS: Record<Theme, string> = { light: "#ffffff", dark: "#14171c" };

export const applyThemeColor = (theme: Theme) => {
  const meta =
    document.head.querySelector('meta[name="theme-color"]') ??
    document.head.appendChild(Object.assign(document.createElement("meta"), { name: "theme-color" }));
  meta.setAttribute("content", THEME_COLORS[theme]);
};

export interface ThemeValue {
  preference: ThemePreference;
  theme: Theme;
}

export const ThemeContext = createContext<ThemeValue>({ preference: "system", theme: "light" });

export const useTheme = () => use(ThemeContext);
