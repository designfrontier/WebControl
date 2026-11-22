import { useState } from "react";
import { useNavigate } from "react-router";
import { CloseButton } from "../../app/CloseButton";
import { useFetch, useMachine } from "../../app/context";
import type { JobQueue } from "../../protocol";
import { selectGcodeLocked } from "../../state/machine";
import { FormError } from "../files/ui";
import { baseName, betweenText, continueLabel, nextText, queueApi, refusal, statusText, useJobQueue } from "./queue";
import "./queue.css";

/** Runs one queue write at a time and shows the server's refusal. */
const useQueueWrite = (update: (queue: JobQueue) => void) => {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const run = (write: () => Promise<JobQueue>, fallback: string) => async () => {
    setBusy(true);
    setError(null);
    try {
      update(await write());
    } catch (failure) {
      setError(refusal(failure, fallback));
    } finally {
      setBusy(false);
    }
  };
  return { busy, error, run };
};

const folderOf = (file: string) => file.slice(0, Math.max(file.lastIndexOf("/"), 0));

/** The control screen's line on the queue: where it stands, what comes next, and Continue once it stopped. */
export const QueueSummary = () => {
  const [queue, update] = useJobQueue();
  const navigate = useNavigate();
  const api = queueApi(useFetch());
  const { busy, error, run } = useQueueWrite(update);
  if (!queue || (queue.items.length === 0 && queue.state !== "stopped")) return null;
  const next = nextText(queue);
  return (
    <section className="queue-summary" aria-label="Job queue" data-state={queue.state}>
      <p role="status">{statusText(queue)}</p>
      {next && <p className="queue-next">{next}</p>}
      <div className="queue-summary-actions">
        {queue.state === "stopped" && queue.items.length > 0 && (
          <button type="button" className="go" disabled={busy} onClick={run(api.resume, "Could not continue.")}>
            Continue queue
          </button>
        )}
        <button type="button" onClick={() => navigate("/queue")}>
          Queue ({queue.items.length})
        </button>
      </div>
      <FormError message={error} />
    </section>
  );
};

/** The queue screen: the files in order, with what happens before each, and the writes. */
export const QueuePage = () => {
  const [queue, update] = useJobQueue();
  const api = queueApi(useFetch());
  const loaded = useMachine((s) => s.gcodeFile);
  const locked = useMachine(selectGcodeLocked);
  const { busy, error, run } = useQueueWrite(update);
  if (!queue) return <p className="muted">Loading the queue…</p>;
  const last = queue.items.length - 1;
  const label = (at: number) =>
    at === 0 ? (queue.headLocked ? "Now" : "First") : at === 1 && queue.headLocked ? "Next" : null;

  return (
    <div className="queue-page">
      <p role="status" className="queue-status" data-state={queue.state}>
        {statusText(queue)}
      </p>
      {queue.items.length === 0 ? (
        <p className="muted">No files queued. Add the loaded file, or choose Add to queue in G-Code ▸ Open.</p>
      ) : (
        <ol className="queue-list" aria-label="Queued files">
          {queue.items.map((item, at) => {
            const fixed = queue.headLocked && at === 0;
            const badge = label(at);
            const folder = folderOf(item.file);
            return (
              <li key={item.id} data-current={fixed}>
                <div className="queue-item-text">
                  <span className="queue-file">{baseName(item.file)}</span>
                  {folder && <span className="muted"> in {folder}</span>}
                  {badge && <span className="queue-badge">{badge}</span>}
                  <span className="queue-between">{fixed ? "Loaded for the job" : betweenText(item)}</span>
                </div>
                <div className="queue-item-actions">
                  <button
                    type="button"
                    aria-label={`Move ${item.file} up`}
                    disabled={busy || fixed || at === 0 || (at === 1 && queue.headLocked)}
                    onClick={run(() => api.move(item.id, at - 1), "Could not move it.")}
                  >
                    Up
                  </button>
                  <button
                    type="button"
                    aria-label={`Move ${item.file} down`}
                    disabled={busy || fixed || at === last}
                    onClick={run(() => api.move(item.id, at + 1), "Could not move it.")}
                  >
                    Down
                  </button>
                  <button
                    type="button"
                    aria-label={`Remove ${item.file}`}
                    disabled={busy || fixed}
                    onClick={run(() => api.remove(item.id), "Could not remove it.")}
                  >
                    Remove
                  </button>
                </div>
              </li>
            );
          })}
        </ol>
      )}
      <FormError message={error} />
      <p className="muted">
        Each file starts when the one before it ends: Z rises to the safe height first, and a file that needs another
        tool, or names none, waits at the tool-change prompt for Resume. {continueLabel(queue)} does the same for the
        first file, from its first line.
      </p>
      <div className="actions">
        <button
          type="button"
          className="primary"
          disabled={busy || locked || queue.items.length === 0 || queue.step !== "none"}
          onClick={run(api.resume, "Could not start the queue.")}
        >
          {continueLabel(queue)}
        </button>
        <button
          type="button"
          disabled={busy || loaded === null}
          onClick={run(() => api.add(loaded ?? ""), "Could not add the loaded file.")}
        >
          Add loaded file
        </button>
        <button type="button" disabled={busy || queue.items.length === 0} onClick={run(api.clear, "Could not clear.")}>
          Clear queue
        </button>
        <CloseButton>Close</CloseButton>
      </div>
    </div>
  );
};
