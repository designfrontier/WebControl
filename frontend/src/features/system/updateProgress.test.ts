import { describe, expect, it } from "vitest";
import { ApiError } from "../../api/http";
import { initialMachineState } from "../../state/machine";
import { updateProgress, updates } from "../../test/systemFixtures";
import { busyReason, changedVersion, outcome, refusalText, runTitle, stepState, stepsFor } from "./updateProgress";

describe("updateProgress", () => {
  it("names each finished run's outcome", () => {
    const done = (over: Parameters<typeof updateProgress>[0]) => outcome(updateProgress({ running: false, ...over }));
    expect(done({ result: "updated" })).toEqual({ text: "Updated from 0.3.0 to 0.4.0.", tone: "ok" });
    expect(done({ result: "upToDate" }).text).toBe("Nothing to do: 0.4.0 is already installed.");
    expect(done({ result: "upToDate", to: null }).text).toBe("Nothing to do: the latest release is already installed.");
    expect(done({ result: "rolledBack" })).toEqual({
      text: "The update to 0.4.0 failed, so 0.3.0 was put back.",
      tone: "warn",
    });
    expect(done({ result: "rolledBack", from: null, to: null }).text).toBe(
      "The update to ? failed, so the previous version was put back.",
    );
    expect(done({ action: "rollback", result: "rolledBack", from: "0.4.0", to: "0.3.0" }).text).toBe(
      "Rolled back from 0.4.0 to 0.3.0.",
    );
    expect(done({ action: "rollback", result: "rolledBack", from: null, to: null }).text).toBe(
      "Rolled back from ? to ?.",
    );
    expect(done({ result: "updated", from: null, to: null }).text).toBe("Updated from ? to ?.");
    expect(done({ result: "rollbackFailed" }).tone).toBe("danger");
    expect(done({ result: "interrupted" }).text).toMatch(/stopped before it finished/);
    expect(done({ result: "failed" }).text).toBe("The update did not run. Nothing was changed.");
    expect(done({ action: "rollback", result: "failed" }).text).toBe("The rollback did not run. Nothing was changed.");
  });

  it("titles a run and says when the page is stale", () => {
    expect(runTitle(updateProgress())).toBe("Updating to v0.4.0");
    expect(runTitle(updateProgress({ to: null }))).toBe("Updating to the latest release");
    expect(runTitle(updateProgress({ action: "rollback", to: "0.2.0" }))).toBe("Rolling back to 0.2.0");
    expect(runTitle(updateProgress({ action: "rollback", to: null }))).toBe("Rolling back to the backup");
    expect(changedVersion(updateProgress({ result: "updated" }))).toBe(true);
    expect(changedVersion(updateProgress({ result: "failed" }))).toBe(false);
  });

  it("marks the steps done up to the running phase, and none while the unit starts", () => {
    const rollback = updateProgress({ action: "rollback", phase: "restarting" });
    const steps = stepsFor(rollback);
    expect(steps.map((step) => step.phase)).toEqual(["checking", "rollingBack", "restarting", "healthCheck"]);
    expect(steps.map((_, i) => stepState(steps, rollback, i))).toEqual(["done", "done", "current", "waiting"]);
    const starting = updateProgress({ phase: "starting" });
    expect(stepsFor(starting).every((_, i, all) => stepState(all, starting, i) === "waiting")).toBe(true);
    const failing = updateProgress({
      phase: "restarting",
      log: ["UPDATE FAILED: rolling back to webcontrol-rs 0.3.0."],
    });
    const after = stepsFor(failing);
    expect(after.at(-2)?.label).toBe("Restarting the previous version");
    expect(stepState(after, failing, after.length - 2)).toBe("current");
    expect(stepState(after, failing, 5)).toBe("done");
  });

  it("is busy from the live run state first, then from the server", () => {
    const idle = { ...initialMachineState, job: { ...initialMachineState.job } };
    expect(busyReason(idle, updates())).toBeNull();
    expect(busyReason(idle, updates({ busy: "Not now: scan." }))).toBe("Not now: scan.");
    expect(busyReason({ ...idle, job: { ...idle.job, inProgress: true } }, updates())).toMatch(/a job is running/);
    expect(busyReason({ ...idle, job: { ...idle.job, probing: true } }, updates())).toMatch(/a Z probe/);
  });

  it("reads the server's refusal, else the failure itself", () => {
    expect(refusalText(new ApiError(409, "POST failed with 409", { error: "Not now." }))).toBe("Not now.");
    expect(refusalText(new ApiError(500, "POST failed with 500"))).toBe("POST failed with 500");
    expect(refusalText(new Error("offline"))).toBe("offline");
    expect(refusalText("odd")).toBe("odd");
  });
});
