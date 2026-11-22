import { describe, expect, it } from "vitest";
import { draftOf, toolInput } from "./draft";
import { toolDetails, toolHeadline, toolStatus } from "./describe";
import { libraryTool } from "./fixtures";

describe("toolStatus", () => {
  const library = [libraryTool(1, '1/4" 2 flute endmill')];

  it("names a library tool by its description, and any other by number", () => {
    expect(toolHeadline(toolStatus(1, library))).toBe('Tool 1 — 1/4" 2 flute endmill');
    expect(toolHeadline(toolStatus(3, library))).toBe("Tool 3 (not in your tool library)");
  });
});

describe("toolDetails", () => {
  it("joins the diameter and flutes that are known", () => {
    expect(toolDetails({ diameter: 6.35, units: "mm", flutes: 2 })).toBe("6.35 mm · 2 flutes");
    expect(toolDetails({ diameter: 0.125, units: "in", flutes: 1 })).toBe("0.125 in · 1 flute");
    expect(toolDetails({ diameter: null, units: null, flutes: null })).toBe("");
  });
});

describe("toolInput", () => {
  const draft = draftOf({ number: 2, description: " v-bit ", diameter: 12.7, units: "mm", flutes: 2, notes: " 60° " });

  it("trims the text and drops units without a diameter", () => {
    expect(toolInput(draft)).toEqual({
      number: 2,
      description: "v-bit",
      diameter: 12.7,
      units: "mm",
      flutes: 2,
      notes: "60°",
    });
    expect(toolInput({ ...draft, diameter: " ", flutes: "" })).toMatchObject({
      diameter: null,
      units: null,
      flutes: null,
    });
    expect(draftOf({ number: 1, description: "x" })).toEqual({
      number: "1",
      description: "x",
      diameter: "",
      units: "mm",
      flutes: "",
      notes: "",
    });
  });

  it("says what to fix first", () => {
    expect(toolInput({ ...draft, number: "0" })).toMatch(/tool number/);
    expect(toolInput({ ...draft, number: "1.5" })).toMatch(/tool number/);
    expect(toolInput({ ...draft, description: "  " })).toMatch(/Describe the tool/);
    expect(toolInput({ ...draft, diameter: "-1" })).toMatch(/diameter/);
    expect(toolInput({ ...draft, diameter: "abc" })).toMatch(/diameter/);
    expect(toolInput({ ...draft, flutes: "2.5" })).toMatch(/Flutes/);
  });
});
