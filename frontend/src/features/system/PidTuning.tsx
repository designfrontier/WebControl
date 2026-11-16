import { useState } from "react";
import { useClient, useFetch, useMachine, useServerMessage } from "../../app/context";
import type { Motor, PidKind, PidSettings, PidTestRequest } from "../../protocol";
import { decimalPoint } from "../../numbers/decimal";
import { selectGcodeLocked, selectRunState } from "../../state/machine";
import { getPidSettings, startPidTest } from "./api";
import { LineChart } from "./LineChart";
import { parsePidMessage } from "./pidData";
import type { PidChart } from "./pidData";
import { errorText, useLoad } from "./useLoad";

const MOTORS: { motor: Motor; name: string }[] = [
  { motor: "L", name: "Top left" },
  { motor: "R", name: "Top right" },
  { motor: "M", name: "Bottom left" },
  { motor: "N", name: "Bottom right" },
];
const motorsFor = (fourMotor: boolean) => (fourMotor ? MOTORS : MOTORS.slice(0, 2));

/** Sign of a clockwise rotateSprocket turn per motor, as the classic PID screen sent it. */
const CLOCKWISE: Record<Motor, 1 | -1> = { L: 1, R: -1, M: -1, N: -1 };
const STEPS = [10, 5, 1, 0.1];

type Field = Exclude<keyof PidTestRequest, "kind" | "motor" | "version">;
const FIELDS: Record<PidKind, { field: Field; label: string }[]> = {
  velocity: [
    { field: "start", label: "Start" },
    { field: "stop", label: "Stop" },
    { field: "steps", label: "Steps" },
    { field: "kp", label: "Kp" },
    { field: "ki", label: "Ki" },
    { field: "kd", label: "Kd" },
  ],
  position: [
    { field: "start", label: "Start" },
    { field: "stop", label: "Stop" },
    { field: "steps", label: "Steps" },
    { field: "time", label: "Time" },
    { field: "kp", label: "Kp" },
    { field: "ki", label: "Ki" },
    { field: "kd", label: "Kd" },
  ],
};

/** Matches api/system.py: the firmware parses plain decimals only. */
const PLAIN_DECIMAL = /^-?(\d+\.?\d*|\.\d+)$/;
const isNumber = (value: string | undefined) => value !== undefined && PLAIN_DECIMAL.test(decimalPoint(value.trim()));

const MotorAdjustment = ({ fourMotor, disabled }: { fourMotor: boolean; disabled: boolean }) => {
  const client = useClient();
  const turns = [
    { name: "Clockwise", sign: 1 },
    { name: "Counter-clockwise", sign: -1 },
  ];
  return (
    <section className="system-card" aria-labelledby="motor-adjust-title">
      <h3 id="motor-adjust-title">Motor adjustment</h3>
      <p className="muted">Runs a motor at full power for the chosen number of seconds, ignoring the encoder.</p>
      <div className="motor-grid">
        {motorsFor(fourMotor).map(({ motor, name }) => (
          <fieldset key={motor} className="motor">
            <legend>{name} motor</legend>
            {turns.map((turn) => (
              <div
                key={turn.name}
                className="motor-row"
                role="group"
                aria-label={`${name} motor ${turn.name.toLowerCase()}`}
              >
                <span>{turn.name}</span>
                {STEPS.map((step) => (
                  <button
                    key={step}
                    type="button"
                    aria-label={`${turn.name} ${step}`}
                    disabled={disabled}
                    onClick={() => client.sendAction("rotateSprocket", motor, CLOCKWISE[motor] * turn.sign * step)}
                  >
                    {step}
                  </button>
                ))}
              </div>
            ))}
          </fieldset>
        ))}
      </div>
    </section>
  );
};

interface FormProps {
  kind: PidKind;
  settings: PidSettings;
  chart: PidChart | undefined;
  disabled: boolean;
}

const PidTestForm = ({ kind, settings, chart, disabled }: FormProps) => {
  const fetchImpl = useFetch();
  const title = kind === "velocity" ? "Velocity PID test" : "Position PID test";
  const [form, setForm] = useState<PidTestRequest>({
    kind,
    motor: "L",
    start: "",
    stop: "",
    steps: "",
    version: "1",
    ...(kind === "position" ? { time: "" } : {}),
    ...settings[kind],
  });
  const [invalid, setInvalid] = useState<Field[]>([]);
  const [status, setStatus] = useState<{ text: string; error: boolean }>({ text: "", error: false });

  const set = (patch: Partial<PidTestRequest>) => setForm((current) => ({ ...current, ...patch }));

  const execute = () => {
    const bad = FIELDS[kind].filter(({ field }) => !isNumber(form[field]));
    setInvalid(bad.map(({ field }) => field));
    if (bad.length) {
      setStatus({ text: `Enter numbers for ${bad.map(({ label }) => label).join(", ")}.`, error: true });
      return;
    }
    setStatus({ text: "Starting test…", error: false });
    const numbers = Object.fromEntries(FIELDS[kind].map(({ field }) => [field, decimalPoint(form[field] ?? "")]));
    startPidTest(fetchImpl, { ...form, ...numbers }).then(
      () => setStatus({ text: "Test sent. Results appear here when the controller finishes.", error: false }),
      (error: unknown) => setStatus({ text: `Could not start the test: ${errorText(error)}`, error: true }),
    );
  };

  return (
    <section className="system-card" aria-labelledby={`${kind}-title`}>
      <h3 id={`${kind}-title`}>{title}</h3>
      <form
        className="pid-form"
        noValidate
        onSubmit={(event) => {
          event.preventDefault();
          execute();
        }}
      >
        <fieldset className="radio-row">
          <legend>Motor</legend>
          {motorsFor(settings.fourMotor).map(({ motor, name }) => (
            <label key={motor}>
              <input
                type="radio"
                name={`${kind}-motor`}
                value={motor}
                checked={form.motor === motor}
                onChange={() => set({ motor })}
              />
              {name}
            </label>
          ))}
        </fieldset>
        <div className="field-grid">
          {FIELDS[kind].map(({ field, label }) => (
            <label key={field} className="field">
              {label}
              <input
                inputMode="decimal"
                value={form[field] ?? ""}
                aria-invalid={invalid.includes(field) || undefined}
                onChange={(event) => set({ [field]: event.target.value })}
              />
            </label>
          ))}
          <label className="field">
            Version
            <select value={form.version} onChange={(event) => set({ version: event.target.value })}>
              <option value="1">1 (error only)</option>
              <option value="2">2 (setpoint, input, output)</option>
            </select>
          </label>
        </div>
        <button type="submit" className="button-primary" disabled={disabled}>
          Run {kind} test
        </button>
        <p className={status.error ? "system-error" : "system-status"} role={status.error ? "alert" : "status"}>
          {status.text}
        </p>
      </form>
      {chart ? (
        <LineChart title={`${title} results`} series={chart.series} />
      ) : (
        <p className="muted">No results yet.</p>
      )}
    </section>
  );
};

export const PidTuning = () => {
  const settings = useLoad(getPidSettings);
  const runState = useMachine(selectRunState);
  const [charts, setCharts] = useState<Partial<Record<PidKind, PidChart>>>({});
  const disabled = runState === "running" || runState === "paused";
  // The server also refuses PID tests while a tool change waits outside a job.
  const testsDisabled = useMachine(selectGcodeLocked) || disabled;

  useServerMessage("updatePIDData", (message) => {
    const chart = parsePidMessage(message);
    if (chart) setCharts((current) => ({ ...current, [chart.kind]: chart }));
  });

  return (
    <section aria-label="PID tuning" className="system-panel">
      <p className="muted">
        Advanced. Running a test saves its Kp, Ki and Kd to the advanced settings. Tests are refused while a job runs.
      </p>
      {settings.status === "loading" && <p role="status">Loading PID settings…</p>}
      {settings.status === "error" && (
        <div className="system-error" role="alert">
          <p>Could not load PID settings: {errorText(settings.error)}</p>
          <button type="button" onClick={settings.reload}>
            Retry
          </button>
        </div>
      )}
      {settings.status === "ready" && (
        <>
          <MotorAdjustment fourMotor={settings.data.fourMotor} disabled={disabled} />
          <div className="pid-tests">
            {(["velocity", "position"] as const).map((kind) => (
              <PidTestForm
                key={kind}
                kind={kind}
                settings={settings.data}
                chart={charts[kind]}
                disabled={testsDisabled}
              />
            ))}
          </div>
        </>
      )}
    </section>
  );
};
