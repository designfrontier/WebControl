import { useId, useState } from "react";
import { Dialog } from "../../app/Dialog";
import { useFetch, useMachine } from "../../app/context";
import type { UpdateAction, UpdateProgress, Updates } from "../../protocol";
import { startUpdate } from "./api";
import { LazyMarkdown } from "./LazyMarkdown";
import { busyReason, changedVersion, outcome, refusalText, runTitle, stepState, stepsFor } from "./updateProgress";
import { when } from "./updates";

const RESTART_NOTE =
  "WebControl restarts while it does this; this page loses the server for a moment and reconnects by itself.";

const STATE_TEXT = { done: "Done", current: "In progress", waiting: "Waiting" } as const;

const Log = ({ log }: { log: string[] }) =>
  log.length > 0 && (
    <details className="update-log">
      <summary>Updater log</summary>
      <pre>{log.join("\n")}</pre>
    </details>
  );

/** The run in progress, step by step; it carries on across WebControl's restart. */
export const UpdateRun = ({ progress, reconnecting }: { progress: UpdateProgress; reconnecting: boolean }) => {
  const steps = stepsFor(progress);
  return (
    <section aria-label="Update progress" className="system-card update-run">
      <h3>{runTitle(progress)}</h3>
      <p>{RESTART_NOTE} Keep this page open to see the result.</p>
      <p role="status" className="system-note">
        {reconnecting
          ? "WebControl is restarting; reconnecting…"
          : progress.phase === "starting"
            ? "Starting the updater…"
            : ""}
      </p>
      <ol className="update-steps-progress">
        {steps.map((step, index) => {
          const state = stepState(steps, progress, index);
          return (
            <li
              key={`${step.phase}-${index}`}
              data-state={state}
              aria-current={state === "current" ? "step" : undefined}
            >
              <span>{step.label}</span>
              <span className="step-state">{STATE_TEXT[state]}</span>
            </li>
          );
        })}
      </ol>
      <Log log={progress.log} />
    </section>
  );
};

/** How the last run from this page ended; `watched` means this page saw it, so its UI may be stale. */
export const LastRun = ({ progress, watched }: { progress: UpdateProgress; watched: boolean }) => {
  const { text, tone } = outcome(progress);
  return (
    <section aria-label="Last update" className="system-card">
      <h3>Last {progress.action === "update" ? "update" : "rollback"} from this page</h3>
      <p className="update-outcome" data-tone={tone} role={watched ? "status" : undefined}>
        {text}
      </p>
      {progress.startedAt !== null && (
        <p className="muted">Started {when(progress.startedAt, { dateStyle: "medium", timeStyle: "short" })}</p>
      )}
      {watched && changedVersion(progress) && (
        <div>
          <button type="button" className="button-primary" onClick={() => window.location.reload()}>
            Reload the page
          </button>
          <p className="muted">The page you have open is still the previous version's.</p>
        </div>
      )}
      <Log log={progress.log} />
    </section>
  );
};

const ConfirmUpdate = ({
  action,
  updates,
  onConfirm,
  onCancel,
}: {
  action: UpdateAction;
  updates: Updates;
  onConfirm: () => void;
  onCancel: () => void;
}) => {
  const { latest, backup, installed } = updates;
  const target = action === "update" ? (latest?.tag ?? "") : (backup?.version ?? "");
  return (
    <Dialog
      title={action === "update" ? `Update to ${target}?` : `Roll back to ${target}?`}
      onClose={onCancel}
      closeButton={false}
      size="medium"
      actions={
        <>
          <button type="button" data-autofocus onClick={onCancel}>
            Cancel
          </button>
          <button
            type="button"
            className={action === "update" ? "button-primary" : "button-danger"}
            onClick={onConfirm}
          >
            {action === "update" ? "Update now" : "Roll back now"}
          </button>
        </>
      }
    >
      {action === "update" ? (
        <>
          <p>
            WebControl downloads {target}, checks it against its SHA-256, backs up this install ({installed.version})
            and installs it. If the new version does not come up, the backup is put back by itself.
          </p>
          <p>{RESTART_NOTE}</p>
          {latest?.notes ? (
            <article className="release" aria-label="Release notes">
              <LazyMarkdown>{latest.notes}</LazyMarkdown>
            </article>
          ) : (
            <p className="muted">This release has no notes.</p>
          )}
        </>
      ) : (
        <>
          <p>
            WebControl puts back {target}, backed up {backup ? when(backup.date) : ""}, in place of {installed.version}.
            Its settings come back with it, so settings changed since then are lost. The backup is kept.
          </p>
          <p>{RESTART_NOTE}</p>
        </>
      )}
    </Dialog>
  );
};

/** "Update to vX" and "Roll back to vY", each confirmed first, and off with the reason while the machine is busy. */
export const UpdateButtons = ({ updates, onStarted }: { updates: Updates; onStarted: (next: Updates) => void }) => {
  const fetchImpl = useFetch();
  const busy = useMachine((state) => busyReason(state, updates));
  const [confirm, setConfirm] = useState<UpdateAction | null>(null);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const busyId = useId();
  const { latest, backup } = updates;
  const offerUpdate = updates.status === "updateAvailable" && latest !== null;
  if (!offerUpdate && !backup) return null;

  const start = (action: UpdateAction) => {
    setConfirm(null);
    setSending(true);
    setError(null);
    startUpdate(fetchImpl, action)
      .then(onStarted, (reason: unknown) => setError(refusalText(reason)))
      .finally(() => setSending(false));
  };
  const blocked = busy !== null || sending;
  return (
    <section aria-label="Update or roll back" className="system-card">
      {busy && (
        <p id={busyId} className="system-note">
          {busy}
        </p>
      )}
      <div className="update-buttons">
        {offerUpdate && (
          <button
            type="button"
            className="button-primary"
            disabled={blocked}
            aria-describedby={busy ? busyId : undefined}
            onClick={() => setConfirm("update")}
          >
            Update to {latest.tag}
          </button>
        )}
        {backup && (
          <button
            type="button"
            disabled={blocked}
            aria-describedby={busy ? busyId : undefined}
            onClick={() => setConfirm("rollback")}
          >
            Roll back to {backup.version}
          </button>
        )}
      </div>
      {error && (
        <p className="system-error" role="alert">
          {error}
        </p>
      )}
      {confirm && (
        <ConfirmUpdate
          action={confirm}
          updates={updates}
          onConfirm={() => start(confirm)}
          onCancel={() => setConfirm(null)}
        />
      )}
    </section>
  );
};
