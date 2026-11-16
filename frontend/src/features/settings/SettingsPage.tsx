import { NavLink, Route, Routes } from "react-router";
import type { SettingsSectionId } from "../../protocol";
import { DisplaySection } from "./DisplaySection";
import { settingsTabs } from "./sections";
import { SettingsForm } from "./SettingsForm";
import { useSettingsSection } from "./useSettingsSection";
import "./settings.css";

const SectionPanel = ({ id, label }: { id: SettingsSectionId; label: string }) => {
  const { load, retry, replace } = useSettingsSection(id);

  if (load.status === "loading") return <p className="muted">Loading settings…</p>;
  if (load.status === "error")
    return (
      <div role="alert" className="settings-load-error">
        <p>Could not load these settings. {load.message}</p>
        <button type="button" onClick={retry}>
          Try again
        </button>
      </div>
    );
  const { section } = load;
  return (
    <div className="settings-section">
      <h3>{label}</h3>
      <div className={id === "gpio" ? "settings-gpio" : undefined}>
        <SettingsForm section={section} onSaved={replace} />
        {id === "gpio" && (
          <figure className="gpio-diagram">
            <img src="/static/images/GPIO/RPI_pin_layout.svg" alt="Raspberry Pi GPIO pin layout" />
            <figcaption className="muted">Pin assignments take effect after WebControl restarts.</figcaption>
          </figure>
        )}
      </div>
    </div>
  );
};

/** Every settings section as tabs in one modal; switching tabs replaces the history entry so Back closes it. */
export const SettingsPage = () => (
  <div className="settings-page">
    <nav aria-label="Settings sections" className="settings-tabs">
      <ul>
        {settingsTabs.map(({ id, path, label }) => (
          <li key={id}>
            <NavLink to={path === "" ? "/settings" : `/settings/${path}`} end replace>
              {label}
            </NavLink>
          </li>
        ))}
        <li>
          <NavLink to="/settings/display" end replace>
            Display
          </NavLink>
        </li>
      </ul>
    </nav>
    <Routes>
      {settingsTabs.map(({ id, path, label }) =>
        path === "" ? (
          <Route key={id} index element={<SectionPanel key={id} id={id} label={label} />} />
        ) : (
          <Route key={id} path={path} element={<SectionPanel key={id} id={id} label={label} />} />
        ),
      )}
      <Route path="display" element={<DisplaySection />} />
      <Route path="*" element={<p>There is no settings section at this address.</p>} />
    </Routes>
  </div>
);
