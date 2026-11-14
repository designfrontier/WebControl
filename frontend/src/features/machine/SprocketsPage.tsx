import { useState } from "react";
import { useClient } from "../../app/context";
import { LoadStatus } from "./LoadStatus";
import { ControllerLog, Screen } from "./Screen";
import { SprocketJog } from "./SprocketJog";
import { DIRECTIONS, EXTEND_COUNTDOWN_SECONDS, motorsFor, sprocketDegrees } from "./sprockets";
import { useCountdown } from "./useCountdown";
import { useMachineSetup } from "./useLoad";

type Chain = "left" | "right";

const RUN_SECONDS = [10, 5, 1, 0.1] as const;

const extendStatus = (chain: Chain, remaining: number) =>
  remaining > 0
    ? `The ${chain} chain starts extending in ${remaining} s. Hold the chain end with some tension so it does not wrap the sprocket.`
    : `The ${chain} chain started extending. Keep tension on it as it feeds off the sprocket.`;

export const SprocketsPage = () => {
  const client = useClient();
  const [setup, retry] = useMachineSetup();
  const [zeroed, setZeroed] = useState(false);
  const [extending, setExtending] = useState<Chain | null>(null);
  const countdown = useCountdown();
  const ready = setup.status === "ready" ? setup.data : null;

  const lockExtend = () => {
    setZeroed(false);
    setExtending(null);
    countdown.reset();
  };

  const extend = (chain: Chain) => {
    client.sendAction("adjustChain", chain);
    setExtending(chain);
    countdown.start(EXTEND_COUNTDOWN_SECONDS);
  };

  const extendLabel = (chain: Chain) => `Extend ${chain} chain${ready ? ` to ${ready.chainExtendLength} mm` : ""}`;

  return (
    <Screen title="Sprockets and chains">
      <LoadStatus load={setup} what="machine setup" onRetry={retry} />
      <h3>Step 1: Set a tooth on each sprocket vertical</h3>
      <p>Turn each sprocket until one tooth points straight up, as precisely as you can.</p>
      <SprocketJog fourMotor={ready?.fourMotor ?? false} onTurn={lockExtend} />

      <h3>Step 2: Define zero</h3>
      <p>With the teeth vertical, tell the controller both chain lengths are zero.</p>
      <button
        type="button"
        className="primary"
        onClick={() => {
          client.sendAction("setSprocketsZero");
          setZeroed(true);
        }}
      >
        Define zero
      </button>

      <h3>Step 3: Extend the chains</h3>
      <p>
        Hook the first link on the top tooth, then extend each chain. Turning a sprocket locks these again until you
        define zero.
      </p>
      <div className="button-grid">
        <button type="button" disabled={!zeroed} onClick={() => extend("left")}>
          {extendLabel("left")}
        </button>
        <button type="button" disabled={!zeroed} onClick={() => extend("right")}>
          {extendLabel("right")}
        </button>
      </div>
      <p role="status" className="extend-status">
        {extending && countdown.remaining !== null ? extendStatus(extending, countdown.remaining) : ""}
      </p>
      <button
        type="button"
        className="danger wide"
        onClick={() => {
          client.sendAction("stopRun");
          setExtending(null);
          countdown.reset();
        }}
      >
        Stop
      </button>

      <details className="advanced">
        <summary>Run motors without encoders</summary>
        <p className="muted">Runs a motor at full speed for a set time, for when its encoder cannot be trusted.</p>
        {motorsFor(false).map((motor) => (
          <div key={motor.id} className="motor-row" role="group" aria-label={`Run ${motor.label.toLowerCase()}`}>
            <span className="motor-row-label">{motor.label}</span>
            {RUN_SECONDS.flatMap((seconds) =>
              DIRECTIONS.map((direction) => (
                <button
                  key={`${direction}-${seconds}`}
                  type="button"
                  aria-label={`Run ${motor.label.toLowerCase()} ${direction} ${seconds} s`}
                  onClick={() => {
                    client.sendAction("rotateSprocket", motor.id, sprocketDegrees(motor, direction, seconds));
                    lockExtend();
                  }}
                >
                  {direction === "clockwise" ? "↻" : "↺"} {seconds} s
                </button>
              )),
            )}
          </div>
        ))}
      </details>
      <ControllerLog />
    </Screen>
  );
};
