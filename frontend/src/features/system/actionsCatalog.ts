import type { ActionCommand, SystemInfo } from "../../protocol";
import type { RunState } from "../../state/machine";

/**
 * processAction checks run state in this order: some commands always run, some also while paused, the rest only idle;
 * "noJob" ones also wait for a tool change outside a job to be resumed.
 */
export type AllowedDuring = "always" | "paused" | "idle" | "noJob";

export interface Confirm {
  title: string;
  body: string;
  confirmLabel: string;
  danger?: boolean;
}

export interface ActionItem {
  id: string;
  label: (info: SystemInfo | null) => string;
  command: ActionCommand;
  arg?: string;
  allowed: AllowedDuring;
  confirm?: Confirm;
  danger?: boolean;
  visible?: (info: SystemInfo | null) => boolean;
}

export interface ActionGroup {
  id: "diagnostics" | "setup" | "controller";
  title: string;
  items: ActionItem[];
}

const fixed = (text: string) => () => text;

const firmwareLabel = (name: string, key: keyof SystemInfo["firmware"]) => (info: SystemInfo | null) => {
  const guided = key === "holey" ? info?.firmwareUpgrade : undefined;
  if (guided) return `Upgrade holey firmware to ${guided.target}…`;
  const version = info?.firmware[key];
  return version ? `Upgrade ${name} firmware (${version})` : `Upgrade ${name} firmware`;
};

const upgrade = (name: string, key: keyof SystemInfo["firmware"], command: ActionCommand): ActionItem => ({
  id: command,
  label: firmwareLabel(name, key),
  command,
  allowed: "idle",
  confirm: {
    title: `Upgrade ${name} firmware?`,
    body: "WebControl flashes the controller. Do not unplug it or close this page until the upgrade reports back.",
    confirmLabel: "Upgrade firmware",
  },
});

const wipe = (id: string, label: string, arg: string, what: string): ActionItem => ({
  id,
  label: fixed(label),
  command: "wipeEEPROM",
  arg,
  allowed: "idle",
  danger: true,
  confirm: {
    title: `${label}?`,
    body: `This erases ${what} on the controller and cannot be undone. WebControl syncs its settings back afterwards.`,
    confirmLabel: label,
    danger: true,
  },
});

export const actionGroups: ActionGroup[] = [
  {
    id: "diagnostics",
    title: "Diagnostics and maintenance",
    items: [
      {
        id: "testMotors",
        label: fixed("Test motors and encoders"),
        command: "testMotors",
        allowed: "idle",
        confirm: {
          title: "Test motors and encoders?",
          body: "Each motor runs at full power for a second in each direction. Keep hands and tools clear of the sled.",
          confirmLabel: "Run motor test",
        },
      },
      { id: "queryCamera", label: fixed("Query camera"), command: "queryCamera", allowed: "always" },
      {
        id: "clearLogs",
        label: fixed("Clear log files"),
        command: "clearLogs",
        allowed: "paused",
        confirm: {
          title: "Clear log files?",
          body: "This empties webcontrol-rs.log and deletes webcontrol-rs.log.1. Download a diagnostics file first if you need them.",
          confirmLabel: "Clear logs",
          danger: true,
        },
      },
      {
        id: "shutdown",
        label: (info) => (info?.canTurnOff ? "Shut down WebControl/WebMCP" : "Shut down WebControl"),
        command: "shutdown",
        allowed: "always",
        danger: true,
        confirm: {
          title: "Shut down WebControl?",
          body: "The server stops and every connected browser loses control of the machine until it is restarted.",
          confirmLabel: "Shut down",
          danger: true,
        },
      },
      {
        id: "TurnOffRPI",
        label: fixed("Turn off Raspberry Pi"),
        command: "TurnOffRPI",
        allowed: "always",
        danger: true,
        visible: (info) => info?.canTurnOff ?? false,
        confirm: {
          title: "Turn off the Raspberry Pi?",
          body: "The computer running WebControl powers down. You will need physical access to turn it back on.",
          confirmLabel: "Turn off",
          danger: true,
        },
      },
    ],
  },
  {
    id: "setup",
    title: "Calibration and setup",
    // Stricter than processAction, which also moves the sled while paused; that loses the paused job's position.
    items: [
      { id: "returnToCenter", label: fixed("Return to center"), command: "returnToCenter", allowed: "idle" },
      {
        id: "resetHomeToCenter",
        label: fixed("Reset home to center"),
        command: "resetHomeToCenter",
        allowed: "noJob",
        confirm: {
          title: "Reset home to center?",
          body: "Home moves to 0, 0 and the loaded gcode shifts with it.",
          confirmLabel: "Reset home",
        },
      },
      { id: "moveToDefault", label: fixed("Check for chain skip"), command: "moveToDefault", allowed: "idle" },
    ],
  },
  {
    id: "controller",
    title: "Controller",
    items: [
      upgrade("stock", "stock", "upgradeStockFirmware"),
      upgrade("holey", "holey", "upgradeHoleyFirmware"),
      upgrade("beta", "custom", "upgradeCustomFirmware"),
      { id: "reportSettings", label: fixed("Sync settings"), command: "reportSettings", allowed: "idle" },
      wipe("wipeSettings", "Wipe controller settings", "Settings", "the settings"),
      wipe("wipeMaslow", "Wipe controller position and settings", "Maslow", "the chain position and settings"),
      wipe("wipeAll", "Wipe controller EEPROM", "All", "everything stored in EEPROM"),
    ],
  },
];

export const isAllowed = (allowed: AllowedDuring, runState: RunState, gcodeLocked = false) =>
  allowed === "always" ||
  ((runState === "idle" || runState === "unknown") && !(allowed === "noJob" && gcodeLocked)) ||
  (allowed === "paused" && runState === "paused");
