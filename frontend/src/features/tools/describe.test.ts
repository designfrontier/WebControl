import { describe, expect, it } from "vitest";
import { draftOf, toolInput } from "./draft";
import { onlyAsksFor, toolDetails, toolHeadline, toolNote, toolStatus } from "./describe";
import { fileTool, libraryTool } from "./fixtures";

describe("toolStatus", () => {
  const library = [libraryTool(1, '1/4" 2 flute endmill', { diameter: 6.35, units: "mm", flutes: 2, notes: "upcut" })];

  it("names a library tool the file does not describe from the library", () => {
    const status = toolStatus(1, library, [fileTool(1, null)]);
    expect(toolHeadline(status)).toBe('Tool 1 — 1/4" 2 flute endmill');
    expect(toolNote(status)).toBeNull();
  });

  it("names a tool neither knows as not in the library", () => {
    const status = toolStatus(3, library, [fileTool(3, null)]);
    expect(toolHeadline(status)).toBe("Tool 3 (not in your tool library)");
    expect(toolNote(status)).toBeNull();
  });

  it("shows the file's description for a tool the library lacks, and suggests adding it", () => {
    const status = toolStatus(6, library, [
      fileTool(6, '6.35 mm (1/4") flat end mill', { diameter: 6.35, units: "mm" }),
    ]);
    expect(toolHeadline(status)).toBe('Tool 6 — 6.35 mm (1/4") flat end mill');
    expect(toolNote(status)).toBe("From this file; not in your tool library.");
    expect(status).toMatchObject({
      kind: "fromFile",
      suggestion: { number: 6, description: '6.35 mm (1/4") flat end mill', diameter: 6.35, units: "mm", flutes: null },
    });
  });

  it("shows the file's description over a different library one, and suggests updating the entry", () => {
    const status = toolStatus(1, library, [fileTool(1, "6 mm flat end mill", { diameter: 6, units: "mm" })]);
    expect(toolHeadline(status)).toBe("Tool 1 — 6 mm flat end mill");
    expect(toolNote(status)).toBe('From this file; your library says 1/4" 2 flute endmill.');
    expect(status).toMatchObject({
      kind: "differs",
      suggestion: { number: 1, description: "6 mm flat end mill", diameter: 6, units: "mm", flutes: 2, notes: "upcut" },
    });
    const unsized = toolStatus(1, library, [fileTool(1, "endmill", { flutes: 3 })]);
    expect(unsized).toMatchObject({ suggestion: { diameter: 6.35, units: "mm", flutes: 3 } });
  });

  it("treats case, spacing and a trailing full stop as the same description", () => {
    const status = toolStatus(1, library, [fileTool(1, '1/4"  2 FLUTE endmill.')]);
    expect(status.kind).toBe("library");
    expect(toolHeadline(status)).toBe('Tool 1 — 1/4"  2 FLUTE endmill.');
  });
});

describe("onlyAsksFor", () => {
  it("matches the firmware's bare insert-tool line for this tool", () => {
    expect(onlyAsksFor("Please insert tool 11", 11)).toBe(true);
    expect(onlyAsksFor(" please insert tool 11. ", 11)).toBe(true);
    expect(onlyAsksFor("Please insert a tool", 4)).toBe(true);
  });

  it("leaves any other text, or another tool's number, alone", () => {
    expect(onlyAsksFor("Please insert tool 1", 11)).toBe(false);
    expect(onlyAsksFor("Please insert tool 11 and zero Z", 11)).toBe(false);
    expect(onlyAsksFor("Insert tool 11", 11)).toBe(false);
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
