import { describe, expect, it } from "vitest";
import { opticalSettings } from "./opticalFixtures";
import { chooseExtents, fieldLabels, isCustomExtents, parseOpticalForm, toForm } from "./opticalForm";

describe("toForm", () => {
  it("turns every setting into field text", () => {
    const form = toForm(opticalSettings({ scaleX: 1.0028 }));
    expect(form.scaleX).toBe("1.0028");
    expect(form.tlX).toBe("-15");
    expect(form.calibrationExtents).toBe("Full Area");
  });

  it("picks the nearest blur size the select offers", () => {
    expect(toForm(opticalSettings({ gaussianBlurValue: 0 })).gaussianBlurValue).toBe("1");
    expect(toForm(opticalSettings({ gaussianBlurValue: 8 })).gaussianBlurValue).toBe("7");
    expect(toForm(opticalSettings({ gaussianBlurValue: 40 })).gaussianBlurValue).toBe("11");
  });

  it("treats an extents name the select does not offer as Custom", () => {
    expect(toForm(opticalSettings({ calibrationExtents: "Rop Right" })).calibrationExtents).toBe("Custom");
  });
});

describe("chooseExtents", () => {
  it("fills in a preset's corners", () => {
    const form = chooseExtents(toForm(opticalSettings()), "Bottom Right");
    expect(form).toMatchObject({ calibrationExtents: "Bottom Right", tlX: "0", tlY: "0", brX: "15", brY: "-7" });
  });

  it("keeps the corners for Custom", () => {
    const form = chooseExtents({ ...toForm(opticalSettings()), tlX: "-3" }, "Custom");
    expect(form).toMatchObject({ calibrationExtents: "Custom", tlX: "-3", brX: "15" });
  });

  it("knows which choices leave the corners editable", () => {
    expect(isCustomExtents("Custom")).toBe(true);
    expect(isCustomExtents("Top Left")).toBe(false);
  });
});

describe("parseOpticalForm", () => {
  it("round-trips valid settings", () => {
    expect(parseOpticalForm(toForm(opticalSettings({ autoScanDirection: 1, gaussianBlurValue: 9 })))).toEqual({
      settings: opticalSettings({ autoScanDirection: 1, gaussianBlurValue: 9 }),
    });
  });

  it("reports non-numbers and corners outside the 31 x 15 grid", () => {
    const form = { ...toForm(opticalSettings()), scaleX: "big", tlX: "-16", brY: "1.5", tlY: "7", feedRate: "" };
    expect(parseOpticalForm(form)).toEqual({
      errors: {
        scaleX: "Enter a number",
        feedRate: "Enter a number",
        tlX: "Enter a whole number from -15 to 15",
        brY: "Enter a whole number from -7 to 7",
      },
    });
  });
});

describe("fieldLabels", () => {
  it("names the fields in form order", () => {
    expect(fieldLabels(["cannyLowValue", "settleTime", "markerY", "calibrationExtents"])).toEqual([
      "Vertical size of marker",
      "Settle time",
      "Canny low value",
    ]);
  });
});
