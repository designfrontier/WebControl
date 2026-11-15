import type { SettingsSectionId } from "../../protocol";

export interface SettingsTab {
  id: SettingsSectionId;
  /** Relative to /settings; the Maslow section is the index. */
  path: string;
  label: string;
}

export const settingsTabs: SettingsTab[] = [
  { id: "maslow", path: "", label: "Maslow" },
  { id: "advanced", path: "advanced", label: "Advanced" },
  { id: "webcontrol", path: "webcontrol", label: "WebControl" },
  { id: "camera", path: "camera", label: "Camera" },
  { id: "gpio", path: "gpio", label: "GPIO" },
];
