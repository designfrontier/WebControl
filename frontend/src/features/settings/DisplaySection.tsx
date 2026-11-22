import { useState } from "react";
import { useFetch, useMachineStore } from "../../app/context";
import { useTheme } from "../../app/themePreference";
import type { ThemePreference } from "../../protocol";
import { saveTheme } from "./api";
import { ControlButtonsSection } from "./ControlButtonsSection";

const THEMES: { value: ThemePreference; label: string }[] = [
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
  { value: "system", label: "System" },
];

/** The display theme, shown at once here and pushed to every other open page; a job does not lock it. */
export const DisplaySection = () => {
  const fetch = useFetch();
  const store = useMachineStore();
  const { preference } = useTheme();
  const [failed, setFailed] = useState(false);

  const show = (theme: ThemePreference) => store.dispatch({ type: "message", command: "theme", data: { theme } });
  const choose = (theme: ThemePreference) => {
    setFailed(false);
    show(theme);
    saveTheme(fetch, theme).catch(() => {
      if (store.getState().theme === theme) show(preference);
      setFailed(true);
    });
  };

  return (
    <div className="settings-section">
      <h3>Display</h3>
      <fieldset className="settings-choice" aria-describedby="theme-note">
        <legend>Display theme</legend>
        {THEMES.map(({ value, label }) => (
          <label key={value}>
            <input
              type="radio"
              name="theme"
              value={value}
              checked={preference === value}
              onChange={() => choose(value)}
            />
            {label}
          </label>
        ))}
        <p id="theme-note" className="muted">
          Applies at once on every device showing WebControl, also during a job. System follows each device's own light
          or dark setting.
        </p>
      </fieldset>
      {failed && (
        <p role="alert" className="setting-error">
          Could not save the theme. Try again.
        </p>
      )}
      <ControlButtonsSection />
    </div>
  );
};
