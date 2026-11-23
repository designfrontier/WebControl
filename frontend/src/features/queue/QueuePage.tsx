import { useState } from "react";
import { useNavigate } from "react-router";
import { CloseButton } from "../../app/CloseButton";
import { useFetch, useMachine } from "../../app/context";
import type { JobQueue } from "../../protocol";
import { selectGcodeLocked } from "../../state/machine";
import { FormError } from "../files/ui";
import {
  baseName,
  betweenText,
  continueLabel,
  nextText,
  queueApi,
  refusal,
  statusText,
  useJobQueue,
  useToolNames,
} from "./queue";
import "./queue.css";

type Run = (write: () => Promise<JobQueue>, fallback: string) => () => Promise<void>;

/** Runs one queue write at a time and shows the server's refusal. */
const useQueueWrite = (apply: (task: () => Promise<JobQueue>) => Promise<void>) => {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const run: Run = (write, fallback) => async () => {
    setBusy(true);
    setError(null);
    try {
      await apply(write);
    } catch (failure) {
      setError(refusal(failure, fallback));
    } finally {
      setBusy(false);
    }
  };
  return { busy, error, run };
};

const folderOf = (file: string) => file.slice(0, Math.max(file.lastIndexOf("/"), 0));

interface ContinueProps {
  queue: JobQueue;
  busy: boolean;
  run: Run;
  disabled?: boolean;
  className: string;
}

/** Continue, or after a Stop part way through the first file, the choice of where it starts again. */
const ContinueButtons = ({ queue, busy, run, disabled = false, className }: ContinueProps) => {
  const api = queueApi(useFetch());
  const off = busy || disabled || queue.items.length === 0 || queue.step !== "none";
  if (queue.stoppedAt === null) {
    return (
      <button type="button" className={className} disabled={off} onClick={run(api.resume, "Could not continue.")}>
        {continueLabel(queue)}
      </button>
    );
  }
  const line = queue.stoppedAt + 1;
  return (
    <>
      <button
        type="button"
        className={className}
        disabled={off}
        onClick={run(() => api.resume("stopped"), "Could not continue.")}
      >
        From line {line} (where it stopped)
      </button>
      <button type="button" disabled={off} onClick={run(() => api.resume("start"), "Could not continue.")}>
        From the start (line 1)
      </button>
    </>
  );
};

/** The control screen's line on the queue: where it stands, what comes next, and Continue once it stopped. */
export const QueueSummary = () => {
  const [queue, update] = useJobQueue();
  const navigate = useNavigate();
  const { busy, error, run } = useQueueWrite(update);
  const tools = useToolNames();
  if (!queue || (queue.items.length === 0 && queue.state !== "stopped")) return null;
  const next = nextText(queue, tools);
  return (
    <section className="queue-summary" aria-label="Job queue" data-state={queue.state}>
      <p role="status">{statusText(queue)}</p>
      {next && <p className="queue-next">{next}</p>}
      <div className="queue-summary-actions">
        {queue.state === "stopped" && queue.items.length > 0 && (
          <ContinueButtons queue={queue} busy={busy} run={run} className="go" />
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
  const tools = useToolNames();
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
                  <span className="queue-badges">
                    {badge && <span className="queue-badge">{badge}</span>}
                    {item.skipBitChanges && <span className="queue-badge queue-override">bit changes skipped</span>}
                  </span>
                  <span className="queue-between">{fixed ? "Loaded for the job" : betweenText(item, tools)}</span>
                  <label className="queue-skip field-inline">
                    <input
                      type="checkbox"
                      checked={item.skipBitChanges}
                      disabled={busy || fixed}
                      onChange={(event) => {
                        const skip = event.target.checked;
                        void run(() => api.skip(item.id, skip), "Could not change the override.")();
                      }}
                    />
                    Don't stop for bit changes in {baseName(item.file)}
                  </label>
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
        Each file starts when the one before it ends, once Z has risen to the safe height. It waits at the tool-change
        prompt first only when the files show another bit, unless its override is on. {continueLabel(queue)} does the
        same for the first file.
      </p>
      <div className="actions">
        <ContinueButtons queue={queue} busy={busy} run={run} disabled={locked} className="primary" />
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
