import { useId, useState } from "react";
import type { ReactNode } from "react";
import { useFetch } from "../../app/context";
import { chainOverSprocketValues } from "../../protocol";
import type { KinematicsType, QuickConfigure } from "../../protocol";
import { getQuickConfigure, saveQuickConfigure } from "./api";
import { LoadStatus } from "./LoadStatus";
import {
  GEARBOX_ALLOWANCE_MM,
  KINEMATICS_OPTIONS,
  fromForm,
  invalidFields,
  subtractGearbox,
  toForm,
  withKinematics,
} from "./quickConfigure";
import type { NumberField, QuickConfigureForm } from "./quickConfigure";
import { Screen } from "./Screen";
import { SetupFigure } from "./SetupFigure";
import { useLoad } from "./useLoad";

const Figure = ({ file, caption }: { file: string; caption: string }) => (
  <SetupFigure folder="quickConfig" file={file} caption={caption} />
);

interface MmFieldProps {
  name: NumberField;
  label: string;
  form: QuickConfigureForm;
  invalid: boolean;
  onChange: (name: NumberField, value: string) => void;
  children?: ReactNode;
}

const MmField = ({ name, label, form, invalid, onChange, children }: MmFieldProps) => {
  const id = useId();
  return (
    <div className="field">
      <label htmlFor={id}>{label} (mm)</label>
      <div className="field-row">
        <input
          id={id}
          type="text"
          inputMode="decimal"
          value={form[name]}
          aria-invalid={invalid}
          aria-describedby={invalid ? `${id}-error` : undefined}
          onChange={(event) => onChange(name, event.target.value)}
        />
        {children}
      </div>
      {invalid && (
        <p id={`${id}-error`} className="field-error">
          Enter a length in mm, zero or more.
        </p>
      )}
    </div>
  );
};

type Save = { status: "idle" } | { status: "saving" } | { status: "saved" } | { status: "error"; message: string };

const QuickConfigureEditor = ({ initial }: { initial: QuickConfigure }) => {
  const fetchImpl = useFetch();
  const kitId = useId();
  const chainId = useId();
  const [form, setForm] = useState(() => toForm(initial));
  const [save, setSave] = useState<Save>({ status: "idle" });
  const [touched, setTouched] = useState(false);
  const invalid = touched ? invalidFields(form) : [];
  const gearboxSpacing = subtractGearbox(form.motorSpacingX);

  const update = (next: QuickConfigureForm) => {
    setForm(next);
    if (save.status !== "saving") setSave({ status: "idle" });
  };
  const setNumber = (name: NumberField, value: string) => update({ ...form, [name]: value });

  const submit = async () => {
    setTouched(true);
    const body = fromForm(form);
    if (!body) return;
    setSave({ status: "saving" });
    try {
      await saveQuickConfigure(fetchImpl, body);
      setSave({ status: "saved" });
    } catch (error) {
      setSave({ status: "error", message: error instanceof Error ? error.message : String(error) });
    }
  };

  return (
    <form
      className="quick-configure"
      noValidate
      onSubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
    >
      <section aria-labelledby={`${kitId}-title`}>
        <h3 id={`${kitId}-title`}>Kinematics kit</h3>
        <p>
          Pick your kit or enter its rotation radius. Quadrilateral kits have no rotation radius, so leave it at zero. A
          triangular kit you set up earlier may come back as Custom; that is fine as long as the radius is right.
        </p>
        <div className="figure-grid">
          {KINEMATICS_OPTIONS.flatMap(({ label, image: file }) =>
            file ? [<Figure key={file} file={file} caption={label} />] : [],
          )}
        </div>
        <div className="field">
          <label htmlFor={kitId}>Kit</label>
          <select
            id={kitId}
            value={form.kinematicsType}
            onChange={(event) => update(withKinematics(form, event.target.value as KinematicsType))}
          >
            {KINEMATICS_OPTIONS.map(({ value, label }) => (
              <option key={value} value={value}>
                {label}
              </option>
            ))}
          </select>
        </div>
        <MmField
          name="rotationRadius"
          label="Rotation radius"
          form={form}
          invalid={invalid.includes("rotationRadius")}
          onChange={setNumber}
        />
      </section>

      <section aria-labelledby={`${chainId}-title`}>
        <h3 id={`${chainId}-title`}>Chain around the sprocket</h3>
        <p>
          Top: the chain comes from the slack side and wraps clockwise around the sprocket to the sled. Bottom: it wraps
          counter-clockwise.
        </p>
        <div className="figure-grid">
          <Figure file="chainofftop.png" caption="Chain off the top" />
          <Figure file="chainoffbottom.png" caption="Chain off the bottom" />
        </div>
        <div className="field">
          <label htmlFor={chainId}>Chain feeds off the</label>
          <select
            id={chainId}
            value={form.chainOverSprocket}
            onChange={(event) =>
              update({ ...form, chainOverSprocket: event.target.value as QuickConfigure["chainOverSprocket"] })
            }
          >
            {chainOverSprocketValues.map((value) => (
              <option key={value} value={value}>
                {value}
              </option>
            ))}
          </select>
        </div>
      </section>

      <section aria-labelledby="motor-spacing-title">
        <h3 id="motor-spacing-title">Distance between motors</h3>
        <p>
          Measure between the centres of the two sprockets. Or measure between the outside edges of the two gearboxes
          and subtract {GEARBOX_ALLOWANCE_MM} mm, which the button below does for you.
        </p>
        <div className="figure-grid">
          <Figure file="measureCenterMotor.png" caption="Sprocket centre to sprocket centre" />
          <Figure file="measureEdgeMotor.png" caption="Gearbox edge to gearbox edge" />
        </div>
        <MmField
          name="motorSpacingX"
          label="Distance between motors"
          form={form}
          invalid={invalid.includes("motorSpacingX")}
          onChange={setNumber}
        >
          <button
            type="button"
            disabled={gearboxSpacing === null}
            onClick={() => gearboxSpacing !== null && setNumber("motorSpacingX", gearboxSpacing)}
          >
            Subtract {GEARBOX_ALLOWANCE_MM} mm
          </button>
        </MmField>
      </section>

      <section aria-labelledby="motor-height-title">
        <h3 id="motor-height-title">Height of motors above the workspace</h3>
        <p>
          Measure roughly how high the motors sit above the top of the workspace. Calibration refines it, but the first
          stage needs it reasonably close.
        </p>
        <div className="figure-grid">
          <Figure file="motorOffsetY.png" caption="Height of motors above the workspace" />
        </div>
        <MmField
          name="motorOffsetY"
          label="Height of motors above workspace"
          form={form}
          invalid={invalid.includes("motorOffsetY")}
          onChange={setNumber}
        />
      </section>

      <div className="form-actions">
        <button type="submit" className="primary" disabled={save.status === "saving"}>
          {save.status === "saving" ? "Saving…" : "Save configuration"}
        </button>
        <p role="status">
          {save.status === "saved"
            ? "Saved. The controller has the new machine dimensions."
            : invalid.length > 0
              ? "Fix the highlighted lengths before saving."
              : ""}
        </p>
        {save.status === "error" && (
          <p role="alert" className="field-error">
            Could not save: {save.message}
          </p>
        )}
      </div>
    </form>
  );
};

export const QuickConfigurePage = () => {
  const [load, retry] = useLoad(getQuickConfigure);
  return (
    <Screen title="Quick configure">
      <p>Enough about your frame for the controller to start calibrating. All lengths are in millimetres.</p>
      <LoadStatus load={load} what="the current configuration" onRetry={retry} />
      {load.status === "ready" && <QuickConfigureEditor initial={load.data} />}
    </Screen>
  );
};
