import { useState } from "react";
import { useFetch } from "../../app/context";
import { Dialog } from "../../app/Dialog";
import { PACING_GAP_MAX_MS, PACING_MODES } from "../../protocol";
import type { PacingMode, PacingSegment } from "../../protocol";
import {
  CUSTOM,
  PRESETS,
  describe,
  pacingLabel,
  presetFor,
  refusal,
  savePacing,
  usePacingStats,
  validGap,
} from "./pacing";
import type { Pacing } from "./pacing";
import "./pacing.css";

const fixed = (value: number | null, digits: number) => (value === null ? "–" : value.toFixed(digits));

const HistoryDialog = ({ history, onClose }: { history: PacingSegment[]; onClose: () => void }) => (
  <Dialog title="Pacing history" onClose={onClose} size="medium">
    {history.length === 0 ? (
      <p>No pacing segment has ended yet. A segment ends when the pacing changes, the job ends or Stop.</p>
    ) : (
      <div className="pacing-history">
        <table>
          <thead>
            <tr>
              <th scope="col">Pacing</th>
              <th scope="col">Lines/min</th>
              <th scope="col">Lines</th>
              <th scope="col">Lost</th>
              <th scope="col">s/line</th>
              <th scope="col">Recovery s</th>
              <th scope="col">Short gap ms</th>
              <th scope="col">Minutes</th>
              <th scope="col">Ended by</th>
            </tr>
          </thead>
          <tbody>
            {history.map((segment, index) => (
              <tr key={index}>
                <th scope="row">{pacingLabel(segment)}</th>
                <td>{fixed(segment.linesPerMinute, 0)}</td>
                <td>{segment.lines}</td>
                <td>
                  {segment.lost} ({fixed(segment.lostPercent, 2)}%)
                </td>
                <td>{fixed(segment.secondsPerLine, 3)}</td>
                <td>{segment.recoveryS.toFixed(1)}</td>
                <td>{fixed(segment.medianShortGapMs, 0)}</td>
                <td>{(segment.elapsedS / 60).toFixed(1)}</td>
                <td>{segment.endedBy}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    )}
  </Dialog>
);

const CustomPacing = ({ current, onApply }: { current: Pacing; onApply: (pacing: Pacing) => void }) => {
  const [mode, setMode] = useState<PacingMode>(current.mode);
  const [gap, setGap] = useState(String(current.gapMs));
  const gapMs = validGap(gap);
  return (
    <form
      className="pacing-custom"
      onSubmit={(event) => {
        event.preventDefault();
        if (gapMs !== null) onApply({ mode, gapMs });
      }}
    >
      <select aria-label="Pacing mode" value={mode} onChange={(event) => setMode(event.target.value as PacingMode)}>
        {PACING_MODES.map((option) => (
          <option key={option} value={option}>
            {option}
          </option>
        ))}
      </select>
      <input
        type="number"
        aria-label="Gap in milliseconds"
        min={0}
        max={PACING_GAP_MAX_MS}
        step={1}
        value={gap}
        aria-invalid={gapMs === null}
        onChange={(event) => setGap(event.target.value)}
      />
      <span aria-hidden="true">ms</span>
      <button type="submit" disabled={gapMs === null}>
        Apply
      </button>
    </form>
  );
};

/** Line pacing for the loss-rate experiment: changeable before Play and mid-job, with the running segment's rate. */
export const PacingControl = () => {
  const fetch = useFetch();
  const [stats, setStats] = usePacingStats();
  const [custom, setCustom] = useState(false);
  const [history, setHistory] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (stats === null) return null;

  const current = { mode: stats.mode, gapMs: stats.gapMs };
  const preset = custom ? CUSTOM : presetFor(current);
  const apply = (pacing: Pacing) =>
    savePacing(fetch, pacing).then(
      (saved) => {
        setStats(saved);
        setError(null);
        setCustom(false);
      },
      (failure: unknown) => setError(refusal(failure)),
    );
  const shown = stats.segment ?? stats.job;
  const summary = shown && describe(shown);

  return (
    <div className="pacing" role="group" aria-label="Line pacing">
      <div className="pacing-row">
        <label htmlFor="line-pacing">Pacing</label>
        <select
          id="line-pacing"
          value={preset}
          onChange={(event) => {
            const chosen = PRESETS.find((option) => option.id === event.target.value);
            setCustom(chosen === undefined);
            if (chosen) void apply(chosen);
          }}
        >
          {PRESETS.map((option) => (
            <option key={option.id} value={option.id}>
              {option.label}
            </option>
          ))}
          <option value={CUSTOM}>
            {preset === CUSTOM && !custom ? `Custom · ${pacingLabel(current)}` : "Custom…"}
          </option>
        </select>
        <button type="button" onClick={() => setHistory(true)}>
          History
        </button>
      </div>
      {custom && <CustomPacing current={current} onApply={(pacing) => void apply(pacing)} />}
      {error !== null && (
        <p className="pacing-error" role="alert">
          {error}
        </p>
      )}
      <p className="pacing-stats" aria-live="polite">
        {summary ? (
          <>
            <strong>{summary.rate}</strong> {stats.segment ? "" : "Last job: "}
            {summary.detail}
          </>
        ) : (
          "No job yet"
        )}
      </p>
      {history && <HistoryDialog history={stats.history} onClose={() => setHistory(false)} />}
    </div>
  );
};
