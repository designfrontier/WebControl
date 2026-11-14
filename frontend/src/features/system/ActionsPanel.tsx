import { useState } from "react";
import { Link } from "react-router";
import { useClient, useMachine } from "../../app/context";
import type { SystemInfo } from "../../protocol";
import { selectRunState } from "../../state/machine";
import { actionGroups, isAllowed } from "./actionsCatalog";
import type { ActionItem } from "./actionsCatalog";
import { downloadUrl } from "./api";
import { ConfirmDialog } from "./Dialog";
import { FakeServoDialog } from "./FakeServoDialog";
import { RestoreDialog } from "./RestoreDialog";

type Open = { kind: "confirm"; item: ActionItem } | { kind: "restore" } | { kind: "fakeServo" } | null;

const DownloadLink = ({ route, children }: { route: "downloadDiagnostics" | "backupWebControl"; children: string }) => (
  <a
    className="button-link"
    href={downloadUrl(route, 0)}
    target="_blank"
    rel="noreferrer"
    onClick={(event) => {
      event.currentTarget.href = downloadUrl(route);
    }}
  >
    {children}
  </a>
);

export const ActionsPanel = ({ info }: { info: SystemInfo | null }) => {
  const client = useClient();
  const runState = useMachine(selectRunState);
  const [open, setOpen] = useState<Open>(null);
  const [notice, setNotice] = useState("");

  const send = (item: ActionItem) => {
    client.sendAction(item.command, item.arg);
    setNotice(`Sent “${item.label(info)}”.`);
    setOpen(null);
  };

  const press = (item: ActionItem) => (item.confirm ? setOpen({ kind: "confirm", item }) : send(item));
  const busy = runState === "running" || runState === "paused";

  return (
    <section aria-labelledby="actions-title" className="system-panel">
      <h2 id="actions-title">Actions</h2>
      {busy && (
        <p className="system-note">
          A job is {runState}. Most actions stay disabled until it is stopped, as the server would refuse them.
        </p>
      )}
      <p className="system-status" role="status">
        {notice}
      </p>
      <div className="action-groups">
        {actionGroups.map((group) => (
          <section key={group.id} className="action-group" aria-label={group.title}>
            <h3>{group.title}</h3>
            {group.items
              .filter((item) => item.visible?.(info) ?? true)
              .map((item) => (
                <button
                  key={item.id}
                  type="button"
                  className={item.danger ? "button-danger" : undefined}
                  disabled={!isAllowed(item.allowed, runState)}
                  onClick={() => press(item)}
                >
                  {item.label(info)}
                </button>
              ))}
            {group.id === "diagnostics" && (
              <>
                <DownloadLink route="downloadDiagnostics">Download diagnostics file</DownloadLink>
                <DownloadLink route="backupWebControl">Back up WebControl</DownloadLink>
                <button type="button" disabled={busy} onClick={() => setOpen({ kind: "restore" })}>
                  Restore WebControl…
                </button>
              </>
            )}
            {group.id === "positioning" && (
              <p className="muted">
                Calibration, sprockets, chains and the z-axis live under <Link to="/calibration">Calibration</Link> and{" "}
                <Link to="/machine">Machine</Link>; settings imports under <Link to="/settings">Settings</Link>.
              </p>
            )}
            {group.id === "controller" && (
              <>
                <Link className="button-link" to="/system/pid">
                  PID tuning (advanced)
                </Link>
                <button type="button" onClick={() => setOpen({ kind: "fakeServo" })}>
                  Fake servo (advanced)…
                </button>
              </>
            )}
          </section>
        ))}
      </div>

      {open?.kind === "confirm" && open.item.confirm && (
        <ConfirmDialog
          title={open.item.confirm.title}
          confirmLabel={open.item.confirm.confirmLabel}
          danger={open.item.confirm.danger}
          onConfirm={() => send(open.item)}
          onCancel={() => setOpen(null)}
        >
          <p>{open.item.confirm.body}</p>
        </ConfirmDialog>
      )}
      {open?.kind === "restore" && (
        <RestoreDialog
          onClose={() => setOpen(null)}
          onRestored={() => {
            setNotice("Backup restored. Settings were reloaded and synced to the controller.");
            setOpen(null);
          }}
        />
      )}
      {open?.kind === "fakeServo" && (
        <FakeServoDialog
          onClose={() => setOpen(null)}
          onSet={(enabled) => {
            client.sendAction("setFakeServo", enabled ? 1 : 0);
            setNotice(
              enabled ? "Asked the controller to enable fake servo." : "Asked the controller to disable fake servo.",
            );
            setOpen(null);
          }}
        />
      )}
    </section>
  );
};
