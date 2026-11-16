import { useState } from "react";
import { ConfirmDialog } from "../../app/Dialog";
import { useFetch } from "../../app/context";
import type { HoleyParameters, OpticalFit } from "../../protocol";
import { formatValue } from "./measurements";
import { acceptOpticalGeometry, fitOpticalGeometry } from "./opticalApi";
import type { Status } from "./StatusLine";

const PARAMETERS: { key: keyof HoleyParameters; label: string }[] = [
  { key: "distanceBetweenMotors", label: "Distance between motors" },
  { key: "motorYoffset", label: "Height of motors" },
  { key: "leftChainTolerance", label: "Left chain tolerance" },
  { key: "rightChainTolerance", label: "Right chain tolerance" },
];

const change = (fit: OpticalFit, key: keyof HoleyParameters) => {
  const delta = fit.fitted[key] - fit.current[key];
  return `${delta > 0 ? "+" : ""}${formatValue(Number(delta.toFixed(4)))}`;
};

/** Fits the holey geometry to the measured grid; nothing is written until the confirm dialog's Write. */
export const OpticalFitPanel = ({ measured, onStatus }: { measured: number; onStatus: (status: Status) => void }) => {
  const fetchImpl = useFetch();
  const [fit, setFit] = useState<OpticalFit | null>(null);
  const [accepted, setAccepted] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);

  const runFit = async () => {
    setBusy(true);
    onStatus({ tone: "info", message: "Fitting the geometry…" });
    const outcome = await fitOpticalGeometry(fetchImpl);
    setBusy(false);
    setFit(outcome.ok ? outcome.data : null);
    setAccepted(false);
    onStatus(
      outcome.ok
        ? { tone: "info", message: "Review the fitted values beside the current ones, then accept to write them." }
        : { tone: "error", message: outcome.error },
    );
  };

  const accept = async (shown: OpticalFit) => {
    setConfirming(false);
    setBusy(true);
    const outcome = await acceptOpticalGeometry(fetchImpl);
    setBusy(false);
    if (outcome.ok) setFit({ ...shown, current: outcome.data });
    setAccepted(outcome.ok);
    onStatus(
      outcome.ok
        ? { tone: "ok", message: "Accepted. The fitted geometry is now in the settings and on the controller." }
        : { tone: "error", message: outcome.error },
    );
  };

  return (
    <div className="card">
      <h3>Fit the machine geometry</h3>
      <p>
        Fits the distance between motors, the motor height and both chain tolerances to the measured squares, allowing
        for where the sheet is pinned. Needs a scan first.
      </p>
      <div className="button-row">
        <button type="button" disabled={measured === 0 || busy} onClick={runFit}>
          Fit geometry
        </button>
        <button
          type="button"
          className="primary"
          disabled={!fit || accepted || busy}
          onClick={() => setConfirming(true)}
        >
          Accept…
        </button>
      </div>
      {fit && (
        <>
          <table className="results">
            <caption className="visually-hidden">Fitted geometry</caption>
            <thead>
              <tr>
                <th scope="col">Setting</th>
                <th scope="col">Current</th>
                <th scope="col">Fitted</th>
                <th scope="col">Change</th>
              </tr>
            </thead>
            <tbody>
              {PARAMETERS.map(({ key, label }) => (
                <tr key={key}>
                  <th scope="row">{label}</th>
                  <td>{formatValue(fit.current[key])}</td>
                  <td>{formatValue(fit.fitted[key])}</td>
                  <td>{change(fit, key)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <p>
            From {fit.cells} squares, RMS residual {fit.rmsMm.toFixed(2)} mm. The sheet sits {fit.sheet.xMm.toFixed(2)}{" "}
            mm right, {fit.sheet.yMm.toFixed(2)} mm up and {fit.sheet.rotationDeg.toFixed(2)}° turned from nominal; that
            is not written anywhere.
          </p>
        </>
      )}
      {confirming && fit && (
        <ConfirmDialog
          title="Write the fitted geometry?"
          confirmLabel="Write settings"
          onConfirm={() => accept(fit)}
          onCancel={() => setConfirming(false)}
        >
          <p>These settings are saved and sent to the controller:</p>
          <ul>
            {PARAMETERS.map(({ key, label }) => (
              <li key={key}>
                {label}: {formatValue(fit.current[key])} → {formatValue(fit.fitted[key])}
              </li>
            ))}
          </ul>
        </ConfirmDialog>
      )}
    </div>
  );
};
