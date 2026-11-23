import { useState } from "react";
import { useFetch, useMachine } from "../../app/context";
import { saveSettingsSection } from "../settings/api";
import { useSettingsSection } from "../settings/useSettingsSection";
import { LIMIT_KEYS, limitText, zInMm } from "./zAxis";
import type { LimitKey } from "./zAxis";

/** The Z travel limits WebControl enforces on every firmware, set from where Z is now. */
export const TravelLimits = () => {
  const fetchImpl = useFetch();
  const { load, replace } = useSettingsSection("maslow");
  const z = useMachine((s) => s.position?.z ?? null);
  const units = useMachine((s) => s.units);
  const running = useMachine((s) => s.job.inProgress);
  const [error, setError] = useState<string | null>(null);
  const here = z === null || units === null ? null : zInMm(z, units);

  const stored = (key: LimitKey) =>
    load.status === "ready" ? load.section.settings.find((setting) => setting.key === key)?.value : undefined;
  const setHere = (key: LimitKey) => {
    if (here === null) return;
    setError(null);
    saveSettingsSection(fetchImpl, "maslow", { [key]: here }).then(replace, (failed: Error) =>
      setError(failed.message),
    );
  };

  return (
    <>
      <h3>Travel limits WebControl keeps</h3>
      <p>
        Jog Z to the highest point the mechanism should reach and set the upper limit, then to the lowest and set the
        lower limit. WebControl refuses every move past them, and a job pauses before the line that would. Define Z0 and
        Touch Z move them with the zero, so they stay at the same place on the mechanism. Change or clear them in
        Settings, Maslow.
      </p>
      <dl className="z-limits">
        <dt>Upper limit</dt>
        <dd>{limitText(stored(LIMIT_KEYS.upper))}</dd>
        <dt>Lower limit</dt>
        <dd>{limitText(stored(LIMIT_KEYS.lower))}</dd>
        <dt>Z now</dt>
        <dd>{here === null ? "Unknown" : `${here} mm`}</dd>
      </dl>
      {error && (
        <p className="notice" data-tone="warn">
          The limit was not saved: {error}
        </p>
      )}
      <div className="button-grid">
        <button type="button" disabled={here === null || running} onClick={() => setHere(LIMIT_KEYS.upper)}>
          Set upper limit here
        </button>
        <button type="button" disabled={here === null || running} onClick={() => setHere(LIMIT_KEYS.lower)}>
          Set lower limit here
        </button>
      </div>
    </>
  );
};
