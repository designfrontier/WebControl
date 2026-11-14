import { useId, useState } from "react";
import type { FormEvent } from "react";
import { useClient, useFetch, useServerMessage } from "../../app/context";
import type { OpticalCameraCommand, OpticalCurve, OpticalError, OpticalSettings, OpticalState } from "../../protocol";
import { getOptical, saveOpticalSettings } from "./api";
import { curveSeries, errorSeries, xyErrorPoints } from "./chartData";
import { LineChart, ScatterChart } from "./Charts";
import { NumberFields, SelectField } from "./OpticalFields";
import {
  BLUR_SIZES,
  CONFIGURATION_FIELDS,
  CORNER_FIELDS,
  EXTENTS,
  RECOGNITION_FIELDS,
  SCAN_DIRECTIONS,
  chooseExtents,
  isCustomExtents,
  parseOpticalForm,
  toForm,
} from "./opticalForm";
import type { OpticalForm, OpticalKey } from "./opticalForm";
import { SledPosition } from "./SledPosition";
import { StatusLine } from "./StatusLine";
import type { Status } from "./StatusLine";
import { useLoad } from "./useLoad";

const CLEARED_CURVE: OpticalCurve = { curveX: [0, 0, 0, 0, 0, 0], curveY: [0, 0, 0, 0, 0, 0] };
const CLEARED_ERROR: OpticalError = { errorX: [], errorY: [] };
const ADJUST_STEP_MM = 5;

const CAMERA_STATUS: Record<OpticalCameraCommand, string> = {
  testOpticalCalibration: "Taking a test image…",
  findCenterOpticalCalibration: "Looking for the center of the marker…",
  optical_Calibrate: "Automatic calibration started. Stop halts it.",
};

const Workbench = ({ initial }: { initial: OpticalState }) => {
  const client = useClient();
  const fetchImpl = useFetch();
  const id = useId();
  const [form, setForm] = useState<OpticalForm>(() => toForm(initial.settings));
  const [errors, setErrors] = useState<Partial<Record<OpticalKey, string>>>({});
  const [curve, setCurve] = useState(initial.curve);
  const [error, setError] = useState(initial.error);
  const [images, setImages] = useState<{ active?: string; test?: string }>({});
  const [status, setStatus] = useState<Status | null>(null);

  useServerMessage("updateOpticalCalibrationCurve", setCurve);
  useServerMessage("updateOpticalCalibrationError", setError);
  useServerMessage("updateOpticalCalibrationFindCenter", ({ opticalCenterX, opticalCenterY }) => {
    setForm((previous) => ({
      ...previous,
      opticalCenterX: opticalCenterX.toFixed(2),
      opticalCenterY: opticalCenterY.toFixed(2),
    }));
    setStatus({ tone: "ok", message: "Found the center. Save the settings to keep it." });
  });
  useServerMessage("updateCalibrationImage", ({ command, data }) =>
    setImages((previous) =>
      command === "OpticalCalibrationImageUpdated" ? { ...previous, active: data } : { ...previous, test: data },
    ),
  );

  const change = (key: OpticalKey, value: string) => setForm((previous) => ({ ...previous, [key]: value }));

  const validSettings = (): OpticalSettings | null => {
    const parsed = parseOpticalForm(form);
    setErrors("errors" in parsed ? parsed.errors : {});
    if ("settings" in parsed) return parsed.settings;
    setStatus({ tone: "error", message: "Check the highlighted settings." });
    return null;
  };

  const runCamera = (command: OpticalCameraCommand) => {
    const settings = validSettings();
    if (!settings) return;
    client.emit("opticalCalibration", { data: { command, arg: settings } });
    setStatus({ tone: "info", message: CAMERA_STATUS[command] });
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

  const clear = () => {
    client.sendAction("clearCalibration");
    setCurve(CLEARED_CURVE);
    setError(CLEARED_ERROR);
  };

  const fields = { form, errors, onChange: change };
  const xy = xyErrorPoints(error.errorX, error.errorY);

  return (
    <>
      <form className="optical-settings" aria-label="Optical calibration settings" noValidate onSubmit={save}>
        <details className="card" open>
          <summary>Configuration</summary>
          <NumberFields fields={CONFIGURATION_FIELDS} {...fields} />
        </details>
        <details className="card">
          <summary>Calibration extents</summary>
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
          <div className="button-row" role="group" aria-labelledby={`${id}-adjust`}>
            <span id={`${id}-adjust`} className="muted">
              Adjust height
            </span>
            <button type="button" onClick={() => client.sendAction("adjustCenter", ADJUST_STEP_MM)}>
              ▲ Up {ADJUST_STEP_MM} mm
            </button>
            <button type="button" onClick={() => client.sendAction("adjustCenter", -ADJUST_STEP_MM)}>
              ▼ Down {ADJUST_STEP_MM} mm
            </button>
          </div>
          <div className="button-row">
            <button type="button" onClick={() => runCamera("testOpticalCalibration")}>
              Test
            </button>
            <button type="button" onClick={() => runCamera("findCenterOpticalCalibration")}>
              Find center
            </button>
          </div>
          <CameraImage label="Test image" data={images.test} />
        </details>
        <div className="button-row">
          <button type="submit" className="primary">
            Save settings
          </button>
        </div>
      </form>

      <div className="card">
        <h3>Calibration</h3>
        <div className="button-row">
          <button type="button" className="primary" onClick={() => runCamera("optical_Calibrate")}>
            Start automatic calibration
          </button>
          <button type="button" className="danger" onClick={() => client.sendAction("stopOpticalCalibration")}>
            Stop
          </button>
          <button type="button" onClick={() => client.sendAction("reloadCalibration")}>
            (Re)load calibration
          </button>
          <button type="button" onClick={clear}>
            Clear calibration
          </button>
          <button type="button" onClick={() => client.sendAction("returnToCenter")}>
            Return to center
          </button>
          <button type="button" onClick={() => client.sendAction("saveCalibrationToCSV")}>
            Save to CSV
          </button>
          <button type="button" className="primary" onClick={() => client.sendAction("saveAndSendOpticalCalibration")}>
            Save and send
          </button>
        </div>
      </div>

      <details className="card">
        <summary>Curve fitting</summary>
        <button type="button" onClick={() => client.sendAction("curveFitOpticalCalibration")}>
          Do curve fit
        </button>
        <div className="chart-grid">
          <LineChart title="X curve" series={curveSeries(curve.curveX)} />
          <LineChart title="Y curve" series={curveSeries(curve.curveY)} />
        </div>
      </details>

      <details className="card">
        <summary>Calibration matrix</summary>
        <div className="chart-grid">
          <LineChart title="X error" series={errorSeries(error.errorX)} />
          <LineChart title="Y error" series={errorSeries(error.errorY)} />
          <ScatterChart title="XY error" ideal={xy.ideal} measured={xy.measured} />
        </div>
      </details>

      <div className="chart-grid">
        <div className="card">
          <h3>Active image</h3>
          <CameraImage label="Latest calibration image" data={images.active} />
        </div>
        <div className="card">
          <h3>Sled position</h3>
          <SledPosition />
        </div>
      </div>
      <StatusLine status={status} />
    </>
  );
};

const CameraImage = ({ label, data }: { label: string; data?: string }) =>
  data ? (
    <img className="camera-image" src={`data:image/png;base64,${data}`} alt={label} />
  ) : (
    <p className="muted">{label}: none yet.</p>
  );

export const OpticalCalibration = () => {
  const id = useId();
  const loaded = useLoad(getOptical);
  return (
    <section className="wizard" aria-labelledby={`${id}-title`}>
      <h2 id={`${id}-title`}>Optical calibration</h2>
      <p className="wizard-intro">
        Uses a camera on the sled and a printed grid of square markers to measure positioning error across the sheet,
        then fits a correction curve and sends it to the controller. Configure and test the camera first.
      </p>
      {loaded.status === "loading" && <p role="status">Loading optical calibration…</p>}
      {loaded.status === "error" && <p role="alert">Could not load the optical calibration settings.</p>}
      {loaded.status === "ready" && <Workbench initial={loaded.data} />}
    </section>
  );
};
