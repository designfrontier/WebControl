import { AboutPanel } from "./AboutPanel";
import { ActionsPanel } from "./ActionsPanel";
import { getSystemInfo } from "./api";
import { LogsPanel } from "./LogsPanel";
import { UpdatesPanel } from "./UpdatesPanel";
import { errorText, useLoad } from "./useLoad";
import type { Load } from "./useLoad";
import type { SystemInfo } from "../../protocol";
import "./system.css";

const InfoError = ({ info }: { info: Load<SystemInfo> }) =>
  info.status === "error" ? (
    <div className="system-error" role="alert">
      <p>Could not load system information: {errorText(info.error)}</p>
      <button type="button" onClick={info.reload}>
        Retry
      </button>
    </div>
  ) : null;

const dataOf = (info: Load<SystemInfo>) => (info.status === "ready" ? info.data : null);

export const ActionsModal = () => {
  const info = useLoad(getSystemInfo);
  return (
    <div className="system">
      <InfoError info={info} />
      <ActionsPanel info={dataOf(info)} />
    </div>
  );
};

export const AboutModal = () => {
  const info = useLoad(getSystemInfo);
  return (
    <div className="system">
      <InfoError info={info} />
      <AboutPanel info={dataOf(info)} />
    </div>
  );
};

export const UpdatesModal = () => (
  <div className="system">
    <UpdatesPanel />
  </div>
);

/** The standalone /logs page the Help menu opens in a new tab: no app chrome. */
export const LogsPage = () => (
  <main className="logs-page system">
    <h1>WebControl logs</h1>
    <LogsPanel />
  </main>
);
