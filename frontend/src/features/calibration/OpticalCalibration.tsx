import { useId, useState } from "react";
import type { FormEvent } from "react";
import { useClient, useFetch, useMachine, useServerMessage } from "../../app/context";
import type { MessageData, OpticalOrientation, OpticalSession, OpticalSettings, OpticalState } from "../../protocol";
import { cellName, gridCells, summarise } from "./chartData";
import { ErrorHeatMap } from "./Charts";
import { OpticalCamera } from "./OpticalCamera";
import { NumberFields, SelectField } from "./OpticalFields";
import { OpticalFitPanel } from "./OpticalFit";
import { loadOptical, opticalCsvUrl, opticalTargetUrl, saveOpticalSettings, startOpticalScan } from "./opticalApi";
import {
  BLUR_SIZES,
  CONFIGURATION_FIELDS,
  CORNER_FIELDS,
  EXTENTS,
  MOTION_FIELDS,
  RECOGNITION_FIELDS,
  SCAN_DIRECTIONS,
  chooseExtents,
  fieldLabels,
  isCustomExtents,
  parseOpticalForm,
  toForm,
} from "./opticalForm";
import type { OpticalForm, OpticalKey } from "./opticalForm";
import { SledPosition } from "./SledPosition";
import { StatusLine } from "./StatusLine";
import type { Status } from "./StatusLine";
import { useLoad } from "./useLoad";

type Progress = MessageData<"opticalScanProgress">;

const STATE_TEXT: Record<Progress["state"], string> = {
  idle: "Idle",
  orienting: "Checking the camera's orientation",
  moving: "Moving",
  settling: "Settling",
  measuring: "Measuring",
  failed: "Waiting: a square failed",
  done: "Done",
  stopped: "Stopped",
};
const RUNNING: readonly Progress["state"][] = ["orienting", "moving", "settling", "measuring", "failed"];
const TERMINAL: readonly Progress["state"][] = ["failed", "done", "stopped"];
const MAX_TURN_DEG = 15;

/** The status line once a scan ends, waits on a failed square or resumes after one. */
const scanOutcome = ({ state, done, total }: Progress, previous?: Progress["state"]): Status | null => {
  const count = `${done} of ${total} squares`;
  if (state === "done") return { tone: "ok", message: `Scan done: ${count}.` };
  if (state === "stopped") return { tone: "info", message: `Scan stopped after ${count}.` };
  if (state === "failed") return { tone: "error", message: `Scan waiting on a failed square after ${count}.` };
  return previous === "failed" ? { tone: "info", message: `Scan resumed after ${count}.` } : null;
};

const firmwareNote = (version: number | null) => {
  if (version === null) return "No controller has reported its firmware yet.";
  if (version >= 50 && version < 100) return `The controller reports holey firmware ${version}, which uses all four.`;
  const family = version < 50 ? "stock" : "custom";
  return `The controller reports ${family} firmware ${version}; the fit models holey firmware, so check the values before accepting.`;
};

const ExperimentalNote = () => {
  const version = useMachine((state) => state.firmwareVersion);
  return (
    <div className="optical-note" role="note" aria-label="Experimental">
      <p>
        <strong className="experimental-badge">Experimental</strong> Not yet tested with a real camera. A scan measures
        how far the sled lands from each square of a printed target. The measurement alone corrects nothing. Only
        Accept, after a fit, writes the distance between motors, the motor height and the chain tolerances.{" "}
        {firmwareNote(version)}
      </p>
    </div>
  );
};

const Downloads = () => (
  <div className="button-row">
    <a className="button-link" href={opticalTargetUrl("letter")} target="_blank" rel="noreferrer">
      Printable target (Letter)
    </a>
    <a className="button-link" href={opticalTargetUrl("a4")} target="_blank" rel="noreferrer">
      Printable target (A4)
    </a>
    <a className="button-link" href={opticalCsvUrl} download>
      Download CSV
    </a>
  </div>
);

const OrientationLine = ({ orientation }: { orientation: OpticalOrientation }) => {
  const wrong = orientation.mirrored || Math.abs(orientation.rotationDeg) > MAX_TURN_DEG;
  return (
    <p className={wrong ? "field-error" : undefined}>
      Camera turned {orientation.rotationDeg.toFixed(1)}°, {orientation.mirrored ? "mirrored" : "not mirrored"},{" "}
      {orientation.pxPerMm.toFixed(2)} px/mm.
      {wrong && ` A scan refuses a mirrored camera or one turned more than ${MAX_TURN_DEG}°; remount it.`}
    </p>
  );
};

const Workbench = ({ initial }: { initial: OpticalState }) => {
  const client = useClient();
  const fetchImpl = useFetch();
  const [form, setForm] = useState<OpticalForm>(() => toForm(initial.settings));
  const [errors, setErrors] = useState<Partial<Record<OpticalKey, string>>>({});
  const [session, setSession] = useState<OpticalSession | null>(initial.session);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [scans, setScans] = useState(0);
  const [orientation, setOrientation] = useState(initial.session?.orientation ?? null);
  const [image, setImage] = useState<MessageData<"opticalImage"> | null>(null);
  const [status, setStatus] = useState<Status | null>(null);

  const refresh = async () => {
    const outcome = await loadOptical(fetchImpl);
    if (outcome.ok) setSession(outcome.data.session);
  };

  useServerMessage("opticalScanProgress", (next) => {
    if (next.done !== progress?.done || TERMINAL.includes(next.state)) void refresh();
    const outcome = next.state === progress?.state ? null : scanOutcome(next, progress?.state);
    if (outcome) setStatus(outcome);
    if (RUNNING.includes(next.state) && !RUNNING.includes(progress?.state ?? "idle")) setScans((n) => n + 1);
    setProgress(next);
  });
  useServerMessage("opticalOrientation", setOrientation);
  useServerMessage("opticalImage", setImage);

  const change = (key: OpticalKey, value: string) => setForm((previous) => ({ ...previous, [key]: value }));

  const validSettings = (): OpticalSettings | null => {
    const parsed = parseOpticalForm(form);
    setErrors("errors" in parsed ? parsed.errors : {});
    if ("settings" in parsed) return parsed.settings;
    setStatus({
      tone: "error",
      message: `Check these settings: ${fieldLabels(Object.keys(parsed.errors)).join(", ")}.`,
    });
    return null;
  };

  const save = async (event: FormEvent) => {
    event.preventDefault();
    const settings = validSettings();
    if (!settings) return;
    setStatus({ tone: "info", message: "Saving…" });
    const outcome = await saveOpticalSettings(fetchImpl, settings);
    if (outcome.ok) setForm(toForm(outcome.data.settings));
    setStatus(outcome.ok ? { tone: "ok", message: "Settings saved." } : { tone: "error", message: outcome.error });
  };

  const scan = async () => {
    const settings = validSettings();
    if (!settings) return;
    const outcome = await startOpticalScan(fetchImpl, settings);
    setStatus(
      outcome.ok
        ? { tone: "info", message: `Scan started: ${outcome.data.total} squares. Stop halts it.` }
        : { tone: "error", message: outcome.error },
    );
  };

  const clear = () => {
    client.sendAction("opticalClearSession");
    setSession(null);
    setProgress(null);
    setImage(null);
  };

  const state = progress?.state ?? session?.state ?? "idle";
  const running = RUNNING.includes(state);
  const cells = gridCells(session?.errorX ?? [], session?.errorY ?? []);
  const fields = { form, errors, onChange: change };

  return (
    <>
      <form className="optical-settings" aria-label="Optical calibration settings" noValidate onSubmit={save}>
        <details className="card" open>
          <summary>Target and camera</summary>
          <NumberFields fields={CONFIGURATION_FIELDS} {...fields} />
        </details>
        <details className="card">
          <summary>Scan extents and motion</summary>
          <SelectField
            label="Calibration extents"
            value={form.calibrationExtents}
            options={EXTENTS}
            onChange={(value) => setForm((previous) => chooseExtents(previous, value))}
          />
          <NumberFields fields={CORNER_FIELDS} disabled={!isCustomExtents(form.calibrationExtents)} {...fields} />
          <SelectField
            label="Auto scan direction"
            value={form.autoScanDirection}
            options={SCAN_DIRECTIONS}
            onChange={(value) => change("autoScanDirection", value)}
          />
          <NumberFields fields={MOTION_FIELDS} {...fields} />
        </details>
        <details className="card">
          <summary>Optical recognition</summary>
          <NumberFields fields={RECOGNITION_FIELDS} {...fields} />
          <SelectField
            label="Gaussian blur"
            value={form.gaussianBlurValue}
            options={BLUR_SIZES}
            onChange={(value) => change("gaussianBlurValue", value)}
          />
        </details>
        <div className="button-row">
          <button type="submit" className="primary">
            Save settings
          </button>
        </div>
      </form>

      <OpticalCamera
        camera={initial.camera}
        settings={validSettings}
        onCentre={(x, y) =>
          setForm((previous) => ({ ...previous, opticalCenterX: x.toFixed(2), opticalCenterY: y.toFixed(2) }))
        }
        onStatus={setStatus}
      />

      <div className="card">
        <h3>Scan</h3>
        <div className="button-row">
          <button type="button" className="primary" disabled={running} onClick={scan}>
            Start scan
          </button>
          <button type="button" className="danger" onClick={() => client.sendAction("stopRun")}>
            Stop
          </button>
          <button type="button" disabled={running} onClick={() => client.sendAction("returnToCenter")}>
            Return to centre
          </button>
          <button type="button" disabled={running || !session} onClick={clear}>
            Clear measurements
          </button>
        </div>
        <p>
          {STATE_TEXT[state]}
          {progress?.cell && `: ${cellName(progress.cell)}, attempt ${progress.attempt}`}
        </p>
        {progress && (
          <progress
            className="scan-progress"
            aria-label="Scan progress"
            value={progress.done}
            max={Math.max(progress.total, 1)}
          />
        )}
        {state === "failed" && (
          <div className="optical-failure" role="alert">
            <p>
              {progress?.cell ? `The square at ${cellName(progress.cell)} failed` : "A square failed"}
              {progress?.error ? `: ${progress.error}` : "."}
            </p>
            <div className="button-row">
              <button type="button" onClick={() => client.sendAction("opticalRetryCell")}>
                Retry square
              </button>
              <button type="button" onClick={() => client.sendAction("opticalSkipCell")}>
                Skip square
              </button>
              <button type="button" className="danger" onClick={() => client.sendAction("stopRun")}>
                Stop scan
              </button>
            </div>
          </div>
        )}
        {orientation && <OrientationLine orientation={orientation} />}
        <ErrorHeatMap cells={cells} current={progress?.cell ?? null} failed={state === "failed"} />
        {image && (
          <img
            className="camera-image"
            src={`data:image/png;base64,${image.png}`}
            alt={`Latest scan image, ${cellName(image.cell)}`}
          />
        )}
        <div className="optical-sled">
          <SledPosition />
        </div>
      </div>

      <OpticalFitPanel key={scans} measured={summarise(cells).measured} onStatus={setStatus} />
      <StatusLine status={status} />
    </>
  );
};

export const OpticalCalibration = () => {
  const id = useId();
  const loaded = useLoad(loadOptical);
  return (
    <section className="wizard" aria-labelledby={`${id}-title`}>
      <h2 id={`${id}-title`}>Optical calibration</h2>
      <ExperimentalNote />
      <Downloads />
      {loaded.status === "loading" && <p role="status">Loading optical calibration…</p>}
      {loaded.status === "ready" && !loaded.data.ok && <p role="alert">{loaded.data.error}</p>}
      {loaded.status === "ready" && loaded.data.ok && <Workbench initial={loaded.data.data} />}
    </section>
  );
};
