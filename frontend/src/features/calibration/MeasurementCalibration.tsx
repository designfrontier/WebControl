import { useId, useState } from "react";
import type { FormEvent, ReactNode } from "react";
import type { Fetch } from "../../api/http";
import { useClient, useFetch } from "../../app/context";
import type { ActionCommand } from "../../protocol";
import type { Outcome } from "./api";
import { formatValue, parseMeasurements } from "./measurements";
import type { MeasurementField } from "./measurements";
import { StatusLine } from "./StatusLine";
import type { Status } from "./StatusLine";
import { useLoad } from "./useLoad";

type Values = Record<string, number>;

/** Everything that differs between the triangular and holey wizards. */
export interface MeasurementConfig {
  title: string;
  intro: ReactNode;
  cutAction: ActionCommand;
  acceptAction: ActionCommand;
  image: { src: string; alt: string };
  fields: MeasurementField[];
  parameters: { key: string; label: string }[];
  load: (fetchImpl: Fetch) => Promise<Values>;
  calculate: (fetchImpl: Fetch, values: Values) => Promise<Outcome<Values>>;
}

const initialText = (fields: MeasurementField[]) =>
  Object.fromEntries(fields.map(({ key, initial }) => [key, initial]));

export const MeasurementCalibration = ({ config }: { config: MeasurementConfig }) => {
  const client = useClient();
  const fetchImpl = useFetch();
  const id = useId();
  const current = useLoad(config.load);
  const [text, setText] = useState(() => initialText(config.fields));
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [result, setResult] = useState<Values | null>(null);
  const [accepted, setAccepted] = useState<Values | null>(null);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<Status | null>(null);

  const calculate = async (event: FormEvent) => {
    event.preventDefault();
    const parsed = parseMeasurements(config.fields, text);
    if ("errors" in parsed) {
      setErrors(parsed.errors);
      setStatus({ tone: "error", message: "Check the highlighted measurements." });
      return;
    }
    setErrors({});
    setBusy(true);
    setStatus({ tone: "info", message: "Calculating…" });
    const outcome = await config.calculate(fetchImpl, parsed.values);
    setBusy(false);
    setResult(outcome.ok ? outcome.data : null);
    setStatus(
      outcome.ok
        ? { tone: "info", message: "Review the calculated values, then accept them to save." }
        : { tone: "error", message: outcome.error },
    );
  };

  const accept = () => {
    client.sendAction(config.acceptAction, 0, 0);
    setAccepted(result);
    setResult(null);
    setStatus({ tone: "ok", message: "Accepted. The calculated values are now the machine settings." });
  };

  const currentValues = accepted ?? (current.status === "ready" ? current.data : undefined);

  return (
    <section className="wizard" aria-labelledby={`${id}-title`}>
      <h2 id={`${id}-title`}>{config.title}</h2>
      <div className="wizard-intro">{config.intro}</div>
      <ol className="wizard-steps">
        <li className="card">
          <h3>Cut the test pattern</h3>
          <p>Load a sheet, set the bit at Z zero and cut the pattern. Stop halts the cut immediately.</p>
          <div className="button-row">
            <button type="button" className="primary" onClick={() => client.sendAction(config.cutAction)}>
              Cut calibration pattern
            </button>
            <button type="button" className="danger" onClick={() => client.sendAction("stopRun")}>
              Stop
            </button>
          </div>
        </li>
        <li className="card">
          <h3>Measure the cuts</h3>
          <img className="diagram" src={config.image.src} alt={config.image.alt} />
          <form className="measurements" aria-label="Measurements" noValidate onSubmit={calculate}>
            <div className="field-grid">
              {config.fields.map(({ key, label }) => (
                <div className="field" key={key}>
                  <label htmlFor={`${id}-${key}`}>{label}</label>
                  <input
                    id={`${id}-${key}`}
                    type="text"
                    inputMode="decimal"
                    value={text[key]}
                    aria-invalid={errors[key] ? true : undefined}
                    aria-describedby={`${id}-${key}-hint`}
                    onChange={(event) => setText((previous) => ({ ...previous, [key]: event.target.value }))}
                  />
                  <small id={`${id}-${key}-hint`} className={errors[key] ? "field-error" : "muted"}>
                    {errors[key] ?? "mm"}
                  </small>
                </div>
              ))}
            </div>
            <button type="submit" className="primary" disabled={busy}>
              Calculate
            </button>
          </form>
        </li>
        <li className="card">
          <h3>Review and accept</h3>
          <table className="results">
            <caption className="visually-hidden">Machine parameters</caption>
            <thead>
              <tr>
                <th scope="col">Parameter</th>
                <th scope="col">Current</th>
                <th scope="col">Calculated</th>
              </tr>
            </thead>
            <tbody>
              {config.parameters.map(({ key, label }) => (
                <tr key={key}>
                  <th scope="row">{label}</th>
                  <td>{formatValue(currentValues?.[key])}</td>
                  <td>{formatValue(result?.[key])}</td>
                </tr>
              ))}
              <tr>
                <th scope="row">Calibration error</th>
                <td>—</td>
                <td>{formatValue(result?.calibrationError)}</td>
              </tr>
            </tbody>
          </table>
          {current.status === "error" && <p className="field-error">Could not load the current settings.</p>}
          <button type="button" className="primary" disabled={!result} onClick={accept}>
            Accept results
          </button>
        </li>
      </ol>
      <StatusLine status={status} />
    </section>
  );
};
