import { describe, expect, it } from "vitest";
import { ALIGNMENTS, alignedCenter, formatNumber, parseNumber } from "./geometry";

const router = { x: 10, y: 5 };
const size = { width: 48, height: 24 };
const align = (label: string) =>
  alignedCenter(
    router,
    size,
    ALIGNMENTS.find((a) => a.label === label)!,
  );

describe("alignedCenter", () => {
  it.each([
    ["Top left", 34, -7],
    ["Top", null, -7],
    ["Top right", -14, -7],
    ["Left", 34, null],
    ["Center", 10, 5],
    ["Right", -14, null],
    ["Bottom left", 34, 17],
    ["Bottom", null, 17],
    ["Bottom right", -14, 17],
  ])("%s puts that part of the board under the router", (label, centerX, centerY) => {
    expect(align(label)).toEqual({ centerX, centerY });
  });

  it("lays the buttons out top to bottom, left to right", () => {
    expect(ALIGNMENTS.map((a) => a.label)).toEqual([
      "Top left",
      "Top",
      "Top right",
      "Left",
      "Center",
      "Right",
      "Bottom left",
      "Bottom",
      "Bottom right",
    ]);
  });
});

describe("parseNumber", () => {
  it.each([
    ["12", 12],
    [" -3.25 ", -3.25],
    ["0", 0],
    ["", null],
    ["  ", null],
    ["abc", null],
    ["1e999", null],
    ["-3,25", -3.25],
    ["1,234.5", null],
  ])("%j -> %s", (text, value) => {
    expect(parseNumber(text)).toBe(value);
  });
});

describe("formatNumber", () => {
  it.each([
    [1219.2, "1219.2"],
    [33.333333, "33.33"],
    [-7, "-7"],
  ])("%s -> %s", (value, text) => {
    expect(formatNumber(value)).toBe(text);
  });
});
