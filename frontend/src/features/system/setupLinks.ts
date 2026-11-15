import type { SystemInfo } from "../../protocol";

export interface SetupLink {
  to: string;
  label: string;
  /** Why the controller's firmware can't use this screen, or null when it can. */
  blockedBy?: (info: SystemInfo | null) => string | null;
}

/** The firmware family by version, as the classic UI decides it: stock below 50, holey below 100, custom above. */
const firmwareFamily = (version: number) => (version < 50 ? "stock" : version < 100 ? "holey" : "custom");

export const reportedFirmware = (info: SystemInfo | null) =>
  info && info.controllerFirmwareVersion > 0
    ? `${firmwareFamily(info.controllerFirmwareVersion)} ${info.firmwareBuild ?? info.controllerFirmwareVersion}`
    : "no version";

const needs = (what: string, info: SystemInfo | null) =>
  `Needs ${what}; the controller reports ${reportedFirmware(info)}.`;

const version = (info: SystemInfo | null) => info?.controllerFirmwareVersion ?? 0;

export const SETUP_LINKS: SetupLink[] = [
  { to: "/files/import/groundcontrol", label: "Import groundcontrol.ini file" },
  { to: "/files/import/webcontrol", label: "Import webcontrol.json file" },
  { to: "/machine/quick-configure", label: "Quick configure" },
  { to: "/machine/sprockets", label: "Set sprockets and chains" },
  { to: "/machine/z-axis", label: "Set Z-axis" },
  {
    to: "/machine/z-limits",
    label: "Set Z-axis limits",
    blockedBy: (info) => (info?.firmwareSupportsZaxisLimit ? null : needs("holey firmware 51.292 or later", info)),
  },
  { to: "/machine/reset-chains", label: "Reset chains" },
  {
    to: "/calibration/triangular",
    label: "Triangular calibration",
    blockedBy: (info) => (version(info) < 50 ? null : needs("stock firmware", info)),
  },
  {
    to: "/calibration/holey",
    label: "Holey calibration",
    blockedBy: (info) => (version(info) >= 50 ? null : needs("holey firmware", info)),
  },
  {
    to: "/calibration/optical",
    label: "Optical calibration",
    blockedBy: (info) => (version(info) >= 100 ? null : needs("custom firmware", info)),
  },
];
