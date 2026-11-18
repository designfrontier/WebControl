import { ApiError } from "../../api/http";
import { errorBodySchema } from "../../protocol";
import type { UpdatePhase, UpdateProgress, Updates } from "../../protocol";
import { selectGcodeLocked, selectProbing } from "../../state/machine";
import type { MachineState } from "../../state/machine";
import type { Tone } from "./updates";

/** How often the page asks for progress while an update runs, and while WebControl restarts. */
export const POLL_MS = 2000;

export interface Step {
  phase: UpdatePhase;
  label: string;
}

const UPDATE_STEPS: Step[] = [
  { phase: "checking", label: "Checking the machine is idle and finding the release" },
  { phase: "downloading", label: "Downloading" },
  { phase: "verified", label: "Checksum OK" },
  { phase: "backingUp", label: "Backing up this install" },
  { phase: "installing", label: "Installing" },
  { phase: "restarting", label: "Restarting WebControl" },
  { phase: "healthCheck", label: "Checking the new version answers" },
];

const ROLLING_BACK: Step = { phase: "rollingBack", label: "Restoring the backup" };

const ROLLBACK_STEPS: Step[] = [
  UPDATE_STEPS[0],
  ROLLING_BACK,
  { phase: "restarting", label: "Restarting WebControl" },
  { phase: "healthCheck", label: "Checking the restored version answers" },
];

/** The steps a run goes through; an update that failed its health check gains the rollback. */
export const stepsFor = ({ action, log }: UpdateProgress): Step[] =>
  action === "rollback"
    ? ROLLBACK_STEPS
    : log.some((line) => line.startsWith("UPDATE FAILED: rolling back"))
      ? [
          ...UPDATE_STEPS,
          ROLLING_BACK,
          { phase: "restarting", label: "Restarting the previous version" },
          { phase: "healthCheck", label: "Checking the previous version answers" },
        ]
      : UPDATE_STEPS;

export type StepState = "done" | "current" | "waiting";

/** Done up to the running phase; while the unit starts, nothing is. */
export const stepState = (steps: Step[], { phase }: UpdateProgress, index: number): StepState => {
  const at = steps.findLastIndex((step) => step.phase === phase);
  return index < at ? "done" : index === at ? "current" : "waiting";
};

export const runTitle = ({ action, to }: UpdateProgress) =>
  action === "update" ? `Updating to ${to ? `v${to}` : "the latest release"}` : `Rolling back to ${to ?? "the backup"}`;

/** What a finished run did, in one line. */
export const outcome = ({ action, result, from, to }: UpdateProgress): { text: string; tone: Tone } => {
  if (result === "updated") return { text: `Updated from ${from ?? "?"} to ${to ?? "?"}.`, tone: "ok" };
  if (result === "upToDate")
    return { text: `Nothing to do: ${to ?? "the latest release"} is already installed.`, tone: "ok" };
  if (result === "rolledBack")
    return action === "rollback"
      ? { text: `Rolled back from ${from ?? "?"} to ${to ?? "?"}.`, tone: "ok" }
      : { text: `The update to ${to ?? "?"} failed, so ${from ?? "the previous version"} was put back.`, tone: "warn" };
  if (result === "rollbackFailed")
    return {
      text: "The rollback failed too. WebControl may not be running properly; see the log below and the README.",
      tone: "danger",
    };
  if (result === "interrupted")
    return { text: "The updater stopped before it finished. See the log below.", tone: "danger" };
  return {
    text:
      action === "update"
        ? "The update did not go ahead; the log below says why."
        : "The rollback did not run. Nothing was changed.",
    tone: "danger",
  };
};

/** Whether this page should reload to load the UI the new version serves. */
export const changedVersion = ({ result }: UpdateProgress) => result === "updated" || result === "rolledBack";

/** Why the buttons are off right now: the live run state first, then the server's last word. */
export const busyReason = (machine: MachineState, updates: Updates) => {
  if (selectProbing(machine)) return "Not now: a Z probe is in flight. Finish or stop it first.";
  if (selectGcodeLocked(machine)) return "Not now: a job is running or paused. Finish or stop it first.";
  return updates.busy;
};

/** The server's refusal, or the request's own failure. */
export const refusalText = (error: unknown) =>
  error instanceof ApiError
    ? (errorBodySchema.safeParse(error.body).data?.error ?? error.message)
    : error instanceof Error
      ? error.message
      : String(error);
