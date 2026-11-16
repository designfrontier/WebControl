import type { Setting, SettingsSection } from "../../protocol";

/** Test data shaped like GET /api/settings/<id>. */
export const setting = (over: Partial<Setting> & Pick<Setting, "key">): Setting => ({
  title: over.key,
  desc: `About ${over.key}`,
  type: "string",
  value: "",
  custom: false,
  numeric: false,
  ...over,
});

export const maslowSection = (over: Partial<SettingsSection> = {}): SettingsSection => ({
  id: "maslow",
  title: "Maslow Settings",
  enableCustom: false,
  ports: ["/dev/ttyACM0", "/dev/ttyACM1"],
  settings: [
    setting({ key: "COMport", title: "Serial Connection", value: "/dev/ttyACM0" }),
    setting({
      key: "motorSpacingX",
      title: "Distance Between Motors",
      desc: "The horizontal distance.\ndefault setting: 2978.4",
      value: "2978.4",
      numeric: true,
    }),
    setting({ key: "zAxis", title: "Z-axis installed", type: "bool", value: 0 }),
    setting({ key: "colorScheme", title: "Color Scheme", type: "options", options: ["Light", "Dark"], value: "Light" }),
    setting({ key: "macro1_title", title: "Macro 1 Title", value: "Macro 1" }),
  ],
  ...over,
});

export const webcontrolSection = (): SettingsSection => ({
  id: "webcontrol",
  title: "WebControl Settings",
  enableCustom: false,
  settings: [
    setting({ key: "loggingTimeout", title: "Logger Idle Timeout", type: "float", value: 0, numeric: true }),
    setting({
      key: "computedPosition",
      title: "Computed Sled Position",
      desc: "Enable on-screen reporting of actual sled position computed from chain lengths reported by controller.",
      type: "bool",
      value: 0,
    }),
    setting({
      key: "uiTheme",
      title: "Display theme",
      type: "options",
      options: ["light", "dark", "system"],
      value: "system",
    }),
  ],
});

export const gpioSection = (): SettingsSection => ({
  id: "gpio",
  title: "GPIO Settings",
  enableCustom: false,
  settings: [2, 3].map((pin) =>
    setting({ key: `gpio${pin}`, title: `GPIO${pin}`, type: "options", options: ["", "Stop", "Pause"], value: "" }),
  ),
});
