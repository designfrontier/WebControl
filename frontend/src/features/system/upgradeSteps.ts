import type { FirmwareUpgrade, UpgradeStage } from "../../protocol";

/** The upgradeHoleyFirmware arg each wizard button sends. */
export type UpgradeStep = "check" | "upgrade" | "selfTest" | "rollback" | "reset";

export interface StepButton {
  step: UpgradeStep;
  label: string;
  danger?: boolean;
  /** A checkbox the owner ticks before the button enables. */
  confirm?: string;
}

export interface StageView {
  title: string;
  /** avrdude or the controller is working; the owner waits and keeps the controller plugged in. */
  busy: boolean;
  buttons: StepButton[];
}

const FLASH_CONFIRM =
  "The router is switched off, the bit is out, and I will not unplug the controller or close WebControl until " +
  "the upgrade reports back.";
const SLACK_CONFIRM = "The chains are slack and the sled is supported, so the motors can turn freely.";

const busy = (title: string): StageView => ({ title, busy: true, buttons: [] });

const rollback = (upgrade: FirmwareUpgrade): StepButton[] =>
  upgrade.canRollback
    ? [{ step: "rollback", label: `Roll back to ${upgrade.firmware ?? "the saved firmware"}`, danger: true }]
    : [];

const views: Record<UpgradeStage, (upgrade: FirmwareUpgrade) => StageView> = {
  idle: (upgrade) => ({
    title: `Upgrade to holey ${upgrade.target}`,
    busy: false,
    buttons: [{ step: "check", label: "Check the controller" }, ...rollback(upgrade)],
  }),
  checking: () => busy("Checking the controller"),
  ready: (upgrade) => ({
    title: "Ready to upgrade",
    busy: false,
    buttons: [
      { step: "upgrade", label: `Upgrade to ${upgrade.target}`, danger: true, confirm: FLASH_CONFIRM },
      { step: "reset", label: "Start over" },
    ],
  }),
  blocked: (upgrade) => ({
    title: "The upgrade is blocked",
    busy: false,
    buttons: [{ step: "check", label: "Check again" }, ...rollback(upgrade)],
  }),
  backingUp: () => busy("Backing up the controller"),
  flashing: () => busy("Flashing the new firmware"),
  restarting: () => busy("Waiting for the controller to boot"),
  migrating: () => busy("Restoring the settings"),
  selfTest: (upgrade) => ({
    title: "Motor self-test",
    busy: false,
    buttons: [{ step: "selfTest", label: "Run the motor self-test", confirm: SLACK_CONFIRM }, ...rollback(upgrade)],
  }),
  testing: () => busy("Testing the motors"),
  done: (upgrade) => ({
    title: `Upgraded to ${upgrade.target}`,
    busy: false,
    buttons: [{ step: "selfTest", label: "Run the self-test again", confirm: SLACK_CONFIRM }, ...rollback(upgrade)],
  }),
  failed: (upgrade) => ({
    title: "The upgrade failed",
    busy: false,
    buttons: [...rollback(upgrade), { step: "reset", label: "Start over" }],
  }),
  rollingBack: () => busy("Restoring the saved firmware"),
  rolledBack: () => ({ title: "Rolled back", busy: false, buttons: [{ step: "reset", label: "Start over" }] }),
};

export const stageView = (upgrade: FirmwareUpgrade): StageView => views[upgrade.stage](upgrade);

export const motorSummary = (passed: boolean[]) =>
  passed.map((ok, i) => `direction ${i + 1} ${ok ? "passed" : "failed"}`).join(", ") || "no result";

export const chainsText = (chains: FirmwareUpgrade["chains"]) =>
  chains ? `left ${chains.left} mm, right ${chains.right} mm` : "not measured";
