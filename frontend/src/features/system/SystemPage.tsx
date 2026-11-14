import { useState } from "react";
import { NavLink, Route, Routes } from "react-router";
import { useServerMessage } from "../../app/context";
import { AboutPanel } from "./AboutPanel";
import { ActionsPanel } from "./ActionsPanel";
import { getSystemInfo } from "./api";
import { HelpPanel } from "./HelpPanel";
import { LogsPanel } from "./LogsPanel";
import { PidTuning } from "./PidTuning";
import { ReleasesPanel } from "./ReleasesPanel";
import { errorText, useLoad } from "./useLoad";
import "./system.css";

const TABS = [
  { to: "", label: "Actions" },
  { to: "pid", label: "PID tuning" },
  { to: "logs", label: "Logs" },
  { to: "help", label: "Help" },
  { to: "updates", label: "Updates" },
  { to: "about", label: "About" },
];

export const SystemPage = () => {
  const info = useLoad(getSystemInfo);
  const [pushedUpdate, setPushedUpdate] = useState(false);
  useServerMessage("pyinstallUpdate", (value) => setPushedUpdate(value === "on"));

  const data = info.status === "ready" ? info.data : null;
  const updateAvailable = pushedUpdate || (data?.update.available ?? false);

  return (
    <section className="system" aria-labelledby="system-title">
      <h1 id="system-title">System</h1>
      <nav aria-label="System" className="subnav">
        <ul>
          {TABS.map(({ to, label }) => (
            <li key={label}>
              <NavLink to={to ? `/system/${to}` : "/system"} end={to === ""}>
                {label}
                {to === "updates" && updateAvailable && (
                  <>
                    {" "}
                    <span className="badge" aria-hidden="true">
                      New
                    </span>
                    <span className="visually-hidden">(update available)</span>
                  </>
                )}
              </NavLink>
            </li>
          ))}
        </ul>
      </nav>
      {info.status === "error" && (
        <div className="system-error" role="alert">
          <p>Could not load system information: {errorText(info.error)}</p>
          <button type="button" onClick={info.reload}>
            Retry
          </button>
        </div>
      )}
      <Routes>
        <Route index element={<ActionsPanel info={data} />} />
        <Route path="pid" element={<PidTuning />} />
        <Route path="logs" element={<LogsPanel />} />
        <Route path="help/*" element={<HelpPanel />} />
        <Route path="updates" element={<ReleasesPanel updateAvailable={updateAvailable} />} />
        <Route path="about" element={<AboutPanel info={data} />} />
        <Route path="*" element={<p>There is no system screen at this address.</p>} />
      </Routes>
    </section>
  );
};
