import { describe, expect, it } from "vitest";
import { fromForm, invalidFields, subtractGearbox, toForm, withKinematics } from "./quickConfigure";
import { DIRECTIONS, SPROCKET_STEPS, motorsFor, sprocketDegrees } from "./sprockets";
import { parseDistance, unitLabel, unitSwitch } from "./zAxis";
import { quickConfigureValues } from "./fixtures";

describe("parseDistance", () => {
  it.each([
    ["1", 1],
    [" 0.25 ", 0.25],
    [".5", 0.5],
    ["2.", 2],
  ])("accepts %j", (text, value) => expect(parseDistance(text)).toBe(value));

  it.each(["", "0", "0.0", "-1", "abc", "1e3", "1.2.3"])("rejects %j", (text) => {
    expect(parseDistance(text)).toBeNull();
  });
});

describe("unitSwitch", () => {
  it("converts the step the way the old unitSwitchZ did", () => {
    expect(unitSwitch("MM", 25.4)).toEqual({ setting: "toInchesZ", value: "1.00" });
    expect(unitSwitch("INCHES", 0.5)).toEqual({ setting: "toMMZ", value: "12.70" });
  });

  it("labels units", () => {
    expect(unitLabel("MM")).toBe("mm");
    expect(unitLabel("INCHES")).toBe("in");
  });
});

describe("sprockets", () => {
  it("lists two motors, or four on four-motor firmware", () => {
    expect(motorsFor(false).map((m) => m.id)).toEqual(["L", "R"]);
    expect(motorsFor(true).map((m) => m.id)).toEqual(["L", "R", "M", "S"]);
  });

  it("matches the old template's degree signs: right-side motors are mirrored", () => {
    const table = motorsFor(true).map((motor) =>
      DIRECTIONS.map((direction) => SPROCKET_STEPS.map((step) => sprocketDegrees(motor, direction, step))),
    );
    expect(table).toEqual([
      [
        [360, 10, 1, 0.1],
        [-360, -10, -1, -0.1],
      ],
      [
        [-360, -10, -1, -0.1],
        [360, 10, 1, 0.1],
      ],
      [
        [360, 10, 1, 0.1],
        [-360, -10, -1, -0.1],
      ],
      [
        [-360, -10, -1, -0.1],
        [360, 10, 1, 0.1],
      ],
    ]);
  });
});

describe("quick configure form", () => {
  const form = toForm(quickConfigureValues());

  it("round-trips values through the text form", () => {
    expect(form.rotationRadius).toBe("138.4");
    expect(fromForm(form)).toEqual(quickConfigureValues());
  });

  it("rejects blank, negative and non-numeric lengths", () => {
    const bad = { ...form, rotationRadius: "", motorSpacingX: "-2", motorOffsetY: "lots" };
    expect(invalidFields(bad)).toEqual(["rotationRadius", "motorSpacingX", "motorOffsetY"]);
    expect(fromForm(bad)).toBeNull();
  });

  it("fills in each kit's rotation radius and leaves Custom alone", () => {
    expect(withKinematics(form, "Linkage").rotationRadius).toBe("273");
    expect(withKinematics(form, "Metal Maslow").rotationRadius).toBe("180.34");
    expect(withKinematics(form, "Quadrilateral").rotationRadius).toBe("0");
    expect(withKinematics({ ...form, rotationRadius: "150" }, "Custom")).toMatchObject({
      kinematicsType: "Custom",
      rotationRadius: "150",
    });
  });

  it("subtracts the gearbox allowance without float noise", () => {
    expect(subtractGearbox("2978.4")).toBe("2938");
    expect(subtractGearbox("3000.15")).toBe("2959.75");
    expect(subtractGearbox("40")).toBeNull();
    expect(subtractGearbox("x")).toBeNull();
  });
});
