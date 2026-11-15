import { useState } from "react";
import { Link } from "react-router";
import { useClient, useMachine } from "../../app/context";
import { ConfirmDialog } from "../../app/Dialog";
import type { SystemInfo } from "../../protocol";
import { selectRunState } from "../../state/machine";
import { actionGroups, isAllowed } from "./actionsCatalog";
import type { ActionItem } from "./actionsCatalog";
import { downloadUrl } from "./api";
import { FakeServoDialog } from "./FakeServoDialog";
import { RestoreDialog } from "./RestoreDialog";
import { SETUP_LINKS } from "./setupLinks";
import type { SetupLink } from "./setupLinks";

type Open = { kind: "confirm"; item: ActionItem } | { kind: "restore" } | { kind: "fakeServo" } | null;

const SetupEntry = ({ link, info }: { link: SetupLink; info: SystemInfo | null }) => {
  const reason = link.blockedBy?.(info);
  if (!reason)
    return (
      <Link className="button-link" to={link.to}>
        {link.label}
      </Link>
    );
  const id = `blocked-${link.to.replaceAll("/", "-")}`;
  return (
    <>
      <button type="button" disabled aria-describedby={id}>
        {link.label}
      </button>
      <p id={id} className="system-note">
        {reason}
      </p>
    </>
  );
};

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
    <section aria-label="Actions" className="system-panel">
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
            {group.id === "setup" && SETUP_LINKS.map((link) => <SetupEntry key={link.to} link={link} info={info} />)}
            {group.id === "controller" && (
              <>
                <Link className="button-link" to="/pid">
                  PID tuning (advanced)
                </Link>
                <button type="button" disabled={busy} onClick={() => setOpen({ kind: "fakeServo" })}>
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
