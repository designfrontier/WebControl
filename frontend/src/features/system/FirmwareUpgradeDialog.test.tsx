import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { FirmwareUpgrade, UpgradeStage } from "../../protocol";
import { topLayer } from "../../test/dialogPolyfill";
import { jsonResponse } from "../../test/fakeFetch";
import { renderApp, renderWithProviders } from "../../test/render";
import { systemFetch, systemInfo } from "../../test/systemFixtures";
import { FirmwareUpgradeDialog } from "./FirmwareUpgradeDialog";
import { chainsText, motorSummary, stageView } from "./upgradeSteps";

const upgrade = (over: Partial<FirmwareUpgrade> = {}): FirmwareUpgrade => ({
  target: "51.292",
  imageSha256: "d7171d97f6eef2735db488a001c556fa60e3639033e6ec7c6ef7b18ff0ee8917",
  stage: "idle",
  message: "",
  board: null,
  firmware: null,
  chains: null,
  chainsAfter: null,
  motors: [],
  backup: null,
  canRollback: false,
  rollbackFirmware: null,
  log: [],
  ...over,
});

const ready = upgrade({
  stage: "ready",
  message: "Ready to upgrade PCB v1.1 Detected to 51.292.",
  board: "PCB v1.1 Detected",
  firmware: "51.27",
  chains: { left: 1650, right: 1651 },
});

/** A fetch whose /api/system/info answers with each upgrade in turn, then keeps the last. */
const sequence = (...steps: FirmwareUpgrade[]) => {
  let at = 0;
  return systemFetch({
    "/api/system/info": () => jsonResponse(systemInfo({ firmwareUpgrade: steps[Math.min(at++, steps.length - 1)] })),
  });
};

const panel = () => within(screen.getByRole("region", { name: "Firmware upgrade" }));

const renderDialog = (initial: FirmwareUpgrade, fetch = sequence(initial)) =>
  renderWithProviders(<FirmwareUpgradeDialog initial={initial} onClose={() => {}} pollMs={5} />, { fetch });

describe("FirmwareUpgradeDialog", () => {
  it("starts with a check of the controller and shows the pinned image", async () => {
    const { user, socket } = renderDialog(upgrade());
    expect(screen.getByRole("heading", { name: "Upgrade to holey 51.292" })).toBeInTheDocument();
    expect(screen.getByText(/d7171d97f6ee/)).toBeInTheDocument();
    expect(panel().getByRole("status")).toHaveTextContent("checks the board and firmware before anything is written");
    await user.click(screen.getByRole("button", { name: "Check the controller" }));
    expect(socket.actions()).toEqual([["upgradeHoleyFirmware", "check", ""]]);
  });

  it("says when the build carries no image", () => {
    renderDialog(upgrade({ imageSha256: null }));
    expect(screen.getByText("none in this build")).toBeInTheDocument();
  });

  it("upgrades only once the owner confirms the router is off and the controller stays plugged in", async () => {
    const { user, socket } = renderDialog(ready);
    expect(screen.getByText("PCB v1.1 Detected")).toBeInTheDocument();
    expect(screen.getByText("left 1650 mm, right 1651 mm")).toBeInTheDocument();
    const go = screen.getByRole("button", { name: "Upgrade to 51.292" });
    expect(go).toBeDisabled();
    await user.click(screen.getByRole("checkbox", { name: /router is switched off/ }));
    await user.click(go);
    expect(socket.actions()).toEqual([["upgradeHoleyFirmware", "upgrade", ""]]);
    await user.click(screen.getByRole("button", { name: "Start over" }));
    expect(socket.actions().at(-1)).toEqual(["upgradeHoleyFirmware", "reset", ""]);
  });

  it("follows the upgrade's progress and warns not to unplug while it works", async () => {
    const flashing = upgrade({ stage: "flashing", message: "Flashing 51.292 (attempt 1).", log: ["Backup saved."] });
    renderDialog(ready, sequence(flashing));
    expect(await screen.findByRole("heading", { name: "Flashing the new firmware" })).toBeInTheDocument();
    expect(panel().getByRole("status")).toHaveTextContent("Flashing 51.292 (attempt 1).");
    expect(screen.getByText("Do not unplug the controller or close WebControl.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Upgrade/ })).not.toBeInTheDocument();
    expect(screen.getByText("Backup saved.")).toBeInTheDocument();
  });

  it("runs the self-test with the chains slack and offers the rollback", async () => {
    const selfTest = upgrade({
      ...ready,
      stage: "selfTest",
      firmware: "51.29",
      canRollback: true,
      rollbackFirmware: "51.27",
      backup: "/home/pi/b/1",
    });
    const { user, socket } = renderDialog(selfTest);
    expect(screen.getByText("/home/pi/b/1")).toBeInTheDocument();
    await user.click(screen.getByRole("checkbox", { name: /chains are slack/ }));
    await user.click(screen.getByRole("button", { name: "Run the motor self-test" }));
    await user.click(screen.getByRole("button", { name: "Roll back to 51.27" }));
    expect(socket.actions()).toEqual([
      ["upgradeHoleyFirmware", "selfTest", ""],
      ["upgradeHoleyFirmware", "rollback", ""],
    ]);
  });

  it("shows each motor's result and the chains measured after", () => {
    renderDialog(
      upgrade({
        stage: "done",
        chainsAfter: { left: 1650, right: 1651 },
        motors: [
          { motor: "L", passed: [true, true] },
          { motor: "Z", passed: [true, false] },
        ],
      }),
    );
    const results = within(screen.getByRole("list", { name: "Motor test" }));
    expect(results.getByText("L: direction 1 passed, direction 2 passed")).toBeInTheDocument();
    expect(results.getByText("Z: direction 1 passed, direction 2 failed")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Upgraded to 51.292" })).toBeInTheDocument();
  });

  it("reports a progress read that fails and recovers when it answers again", async () => {
    let calls = 0;
    const fetch = systemFetch({
      "/api/system/info": () =>
        calls++ === 0 ? jsonResponse({ error: "down" }, 500) : jsonResponse(systemInfo({ firmwareUpgrade: ready })),
    });
    renderDialog(upgrade(), fetch);
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not read the upgrade's progress");
    await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
    expect(await screen.findByRole("heading", { name: "Ready to upgrade" })).toBeInTheDocument();
  });

  it("keeps the last progress when the backend stops reporting an upgrade", async () => {
    renderDialog(ready, systemFetch({ "/api/system/info": systemInfo() }));
    await new Promise((resolve) => setTimeout(resolve, 30));
    expect(screen.getByRole("heading", { name: "Ready to upgrade" })).toBeInTheDocument();
  });
});

describe("upgradeSteps", () => {
  const stages: UpgradeStage[] = [
    "idle",
    "checking",
    "ready",
    "blocked",
    "backingUp",
    "flashing",
    "restarting",
    "migrating",
    "selfTest",
    "testing",
    "done",
    "failed",
    "rollingBack",
    "rolledBack",
  ];

  it("gives every stage a title, and buttons only when nothing is running", () => {
    stages.forEach((stage) => {
      const view = stageView(upgrade({ stage }));
      expect(view.title).not.toBe("");
      expect(view.busy && view.buttons.length > 0).toBe(false);
    });
  });

  it("offers the rollback wherever a saved image can go back on", () => {
    const rollbacks = stages.filter((stage) =>
      stageView(upgrade({ stage, canRollback: true })).buttons.some((b) => b.step === "rollback"),
    );
    expect(rollbacks).toEqual(["idle", "blocked", "selfTest", "done", "failed"]);
    expect(stageView(upgrade({ stage: "failed", canRollback: true })).buttons[0].label).toBe(
      "Roll back to the saved firmware",
    );
  });

  it("names the version read off the board before the upgrade, not the one running now", () => {
    const after = upgrade({ stage: "blocked", firmware: "51.29", canRollback: true, rollbackFirmware: "51.27" });
    expect(stageView(after).buttons.map((b) => b.label)).toContain("Roll back to 51.27");
  });

  it("words motor results and chains", () => {
    expect(motorSummary([])).toBe("no result");
    expect(chainsText(null)).toBe("not measured");
  });
});

describe("the Actions screen with webcontrol-rs", () => {
  it("opens the guided upgrade in place of the one-click flash", async () => {
    const fetch = systemFetch({ "/api/system/info": systemInfo({ firmwareUpgrade: upgrade() }) });
    const { user, socket } = renderApp("/actions", { fetch });
    await user.click(await screen.findByRole("button", { name: "Upgrade holey firmware to 51.292…" }));
    const wizard = topLayer().at(-1) as HTMLElement;
    expect(wizard).toHaveAccessibleName("Upgrade holey firmware");
    expect(socket.actions()).toEqual([]);
    await user.click(within(wizard).getByRole("button", { name: "Close" }));
    expect(topLayer()).toHaveLength(1);
  });
});
