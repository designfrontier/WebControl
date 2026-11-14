import { describe, expect, it } from "vitest";
import { settingsSectionSchema } from "../../protocol";
import { maslowSection, setting } from "./fixtures";
import {
  changedValues,
  controlFor,
  displayDesc,
  initialValues,
  isChecked,
  isLocked,
  optionLabel,
  portOptions,
  selectOptions,
  validate,
  validateValue,
} from "./form";

describe("initialValues", () => {
  it("turns saved values into form values", () => {
    expect(initialValues(maslowSection().settings)).toEqual({
      COMport: "/dev/ttyACM0",
      motorSpacingX: "2978.4",
      zAxis: false,
      colorScheme: "Light",
      macro1_title: "Macro 1",
    });
  });

  it.each([
    [1, true],
    ["1", true],
    ["on", true],
    [true, true],
    [0, false],
    ["off", false],
    ["", false],
  ])("reads bool value %j as %s", (value, checked) => {
    expect(isChecked(value)).toBe(checked);
  });

  it("stringifies numbers saved by typed settings", () => {
    expect(initialValues([setting({ key: "webPort", type: "int", value: 5000 })])).toEqual({ webPort: "5000" });
  });
});

describe("displayDesc", () => {
  it("turns escaped newlines into real ones", () => {
    expect(displayDesc("The path\\ndefault setting: home")).toBe("The path\ndefault setting: home");
    expect(displayDesc("a\nb")).toBe("a\nb");
  });
});

describe("controlFor", () => {
  it.each([
    [setting({ key: "zAxis", type: "bool" }), "checkbox"],
    [setting({ key: "COMport" }), "port"],
    [setting({ key: "fPWM", type: "options", options: ["490Hz"] }), "select"],
    [setting({ key: "broken", type: "options" }), "text"],
    [setting({ key: "openFile", type: "path" }), "text"],
    [setting({ key: "fps", type: "float" }), "text"],
  ])("renders %j as %s", (input, control) => {
    expect(controlFor(input)).toBe(control);
  });
});

describe("options", () => {
  it("keeps the saved port when the scan did not find it", () => {
    expect(portOptions("COM3", ["COM4"])).toEqual(["COM3", "COM4"]);
    expect(portOptions("COM4", ["COM4"])).toEqual(["COM4"]);
  });

  it("keeps an unset port selectable", () => {
    expect(portOptions("", [])).toEqual([""]);
    expect(portOptions("", ["COM4"])).toEqual(["", "COM4"]);
  });

  it("keeps a saved option the schema no longer lists", () => {
    const fPWM = setting({ key: "fPWM", type: "options", options: ["490Hz", "4,100Hz"] });
    expect(selectOptions(fPWM, "490Hz")).toEqual(["490Hz", "4,100Hz"]);
    expect(selectOptions(fPWM, "31,000Hz")).toEqual(["31,000Hz", "490Hz", "4,100Hz"]);
    expect(selectOptions(setting({ key: "x", type: "options" }), "a")).toEqual(["a"]);
  });

  it("labels the empty option", () => {
    expect(optionLabel("", setting({ key: "COMport" }))).toBe("Not set");
    expect(optionLabel("", setting({ key: "gpio2" }))).toBe("Unassigned");
    expect(optionLabel("Stop", setting({ key: "gpio2" }))).toBe("Stop");
  });
});

describe("validation", () => {
  const numeric = setting({ key: "homeX", numeric: true });
  const whole = setting({ key: "webPort", type: "int", numeric: true });

  it.each(["12", "-3.5", ".5", "1e-6", " 7 ", "+2"])("accepts number %j", (value) => {
    expect(validateValue(numeric, value)).toBeUndefined();
  });

  it.each(["", "abc", "1.2.3", "1,5", "-"])("rejects number %j", (value) => {
    expect(validateValue(numeric, value)).toBe("Enter a number");
  });

  it("wants whole numbers for int settings", () => {
    expect(validateValue(whole, "5000")).toBeUndefined();
    expect(validateValue(whole, "50.5")).toBe("Enter a whole number");
  });

  it("accepts any text or bool otherwise", () => {
    expect(validateValue(setting({ key: "macro1" }), "")).toBeUndefined();
    expect(validateValue(setting({ key: "zAxis", type: "bool" }), true)).toBeUndefined();
  });

  it("collects errors for editable settings only", () => {
    const section = maslowSection({
      settings: [numeric, { ...whole, custom: true }],
    });
    expect(validate(section, { homeX: "x", webPort: "y" })).toEqual({ homeX: "Enter a number" });
    expect(validate({ ...section, enableCustom: true }, { homeX: "1", webPort: "y" })).toEqual({
      webPort: "Enter a whole number",
    });
  });

  it("leaves settings that were not changed alone", () => {
    expect(validate(maslowSection({ settings: [numeric, whole] }), { webPort: "80" })).toEqual({});
  });
});

describe("changedValues", () => {
  it("sends only what changed, trimming numbers", () => {
    const section = maslowSection();
    const initial = initialValues(section.settings);
    const values = { ...initial, motorSpacingX: " 3000 ", zAxis: true, macro1_title: " Spindle " };
    expect(changedValues(section, initial, values)).toEqual({
      motorSpacingX: "3000",
      zAxis: true,
      macro1_title: " Spindle ",
    });
    expect(changedValues(section, initial, initial)).toEqual({});
  });

  it("never sends locked custom settings", () => {
    const section = maslowSection({ settings: [setting({ key: "macro2", custom: true, value: "a" })] });
    expect(isLocked(section.settings[0], false)).toBe(true);
    expect(changedValues(section, { macro2: "a" }, { macro2: "b" })).toEqual({});
    expect(changedValues({ ...section, enableCustom: true }, { macro2: "a" }, { macro2: "b" })).toEqual({
      macro2: "b",
    });
  });
});

describe("settingsSectionSchema", () => {
  it("treats a null saved value as empty", () => {
    const parsed = settingsSectionSchema.parse({
      ...maslowSection(),
      settings: [{ ...setting({ key: "openFile" }), value: null }],
    });
    expect(parsed.settings[0].value).toBe("");
  });
});
