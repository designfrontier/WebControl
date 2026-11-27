import { useEffect, useState } from "react";
import { Link } from "react-router";
import { useClient, useFetch } from "../../app/context";
import { Dialog } from "../../app/Dialog";
import type { FirmwareUpgrade } from "../../protocol";
import { getSystemInfo } from "./api";
import { errorText } from "./useLoad";
import { PLANNER_TARGET, chainsText, motorSummary, stageView } from "./upgradeSteps";
import type { StepButton } from "./upgradeSteps";

interface Props {
  initial: FirmwareUpgrade;
  onClose: () => void;
  /** How often the upgrade's progress is read back. */
  pollMs?: number;
}

/** Follows the upgrade through GET /api/system/info, which carries its stage while webcontrol-rs runs it. */
const useUpgrade = (initial: FirmwareUpgrade, pollMs: number) => {
  const fetchImpl = useFetch();
  const [upgrade, setUpgrade] = useState(initial);
  const [error, setError] = useState("");
  useEffect(() => {
    let live = true;
    const poll = () =>
      getSystemInfo(fetchImpl).then(
        (info) => {
          if (!live) return;
          setError("");
          if (info.firmwareUpgrade) setUpgrade(info.firmwareUpgrade);
        },
        (failure: unknown) => live && setError(errorText(failure)),
      );
    const timer = setInterval(poll, pollMs);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [fetchImpl, pollMs]);
  return { upgrade, error };
};

const Fact = ({ name, value }: { name: string; value: string | null }) =>
  value === null ? null : (
    <>
      <dt>{name}</dt>
      <dd>{value}</dd>
    </>
  );

const StepAction = ({ button, onSend }: { button: StepButton; onSend: (button: StepButton) => void }) => {
  const [ticked, setTicked] = useState(false);
  return (
    <div className="upgrade-step">
      {button.confirm && (
        <label className="upgrade-confirm">
          <input type="checkbox" checked={ticked} onChange={(event) => setTicked(event.target.checked)} />
          {button.confirm}
        </label>
      )}
      <button
        type="button"
        className={button.danger ? "button-danger" : undefined}
        disabled={Boolean(button.confirm) && !ticked}
        onClick={() => onSend(button)}
      >
        {button.label}
      </button>
    </div>
  );
};

export const FirmwareUpgradeDialog = ({ initial, onClose, pollMs = 1000 }: Props) => {
  const client = useClient();
  const { upgrade, error } = useUpgrade(initial, pollMs);
  const view = stageView(upgrade);
  const send = (button: StepButton) => client.sendAction("upgradeHoleyFirmware", button.step);

  return (
    <Dialog title="Upgrade holey firmware" onClose={onClose} size="medium">
      <section className="upgrade" aria-label="Firmware upgrade">
        <h3>{view.title}</h3>
        <p role="status" className={view.busy ? "system-note" : undefined}>
          {upgrade.message || "WebControl checks the board and firmware before anything is written."}
        </p>
        {view.busy && <p className="system-note">Do not unplug the controller or close WebControl.</p>}
        <p className="muted">
          The planner firmware {PLANNER_TARGET} is experimental: it plans each move's acceleration and takes corners
          without stopping, and talks at 115200 baud. Holey 51.292 is the way back: upgrade to it here at any time.{" "}
          <Link to="/help/Actions/Controller/upgradeHoleyFirmware.md">Help</Link>
        </p>
        {error && (
          <p className="system-error" role="alert">
            Could not read the upgrade's progress: {error}
          </p>
        )}
        <dl className="facts upgrade-facts">
          <Fact name="Board" value={upgrade.board} />
          <Fact name="Firmware before" value={upgrade.firmware} />
          <Fact name="Chains before" value={upgrade.chains && chainsText(upgrade.chains)} />
          <Fact name="Chains after" value={upgrade.chainsAfter && chainsText(upgrade.chainsAfter)} />
          <Fact name={`${upgrade.target} image SHA-256`} value={upgrade.imageSha256 ?? "none in this build"} />
          <Fact name="Backup" value={upgrade.backup} />
        </dl>
        {upgrade.motors.length > 0 && (
          <ul aria-label="Motor test">
            {upgrade.motors.map((motor) => (
              <li key={motor.motor}>
                {motor.motor}: {motorSummary(motor.passed)}
              </li>
            ))}
          </ul>
        )}
        <div className="upgrade-steps">
          {view.buttons.map((button) => (
            <StepAction key={`${upgrade.stage}-${button.step}`} button={button} onSend={send} />
          ))}
        </div>
        {upgrade.log.length > 0 && (
          <details>
            <summary>Steps so far</summary>
            <ol className="upgrade-log">
              {upgrade.log.map((line, i) => (
                <li key={i}>{line}</li>
              ))}
            </ol>
          </details>
        )}
      </section>
    </Dialog>
  );
};
