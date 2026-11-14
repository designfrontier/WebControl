import { describe, expect, it } from "vitest";
import { formatValue, parseMeasurements, parseNumber } from "./measurements";

const fields = [
  { key: "a", label: "A", initial: "1" },
  { key: "b", label: "B", initial: "2" },
];

describe("parseNumber", () => {
  it.each([
    ["12.5", 12.5],
    [" -3 ", -3],
    ["0", 0],
    ["", null],
    ["  ", null],
    ["abc", null],
    ["Infinity", null],
    ["1e400", null],
  ])("parses %j as %j", (text, expected) => {
    expect(parseNumber(text)).toBe(expected);
  });
});

describe("parseMeasurements", () => {
  it("returns every value when all fields are numbers", () => {
    expect(parseMeasurements(fields, { a: "1.5", b: "2" })).toEqual({ values: { a: 1.5, b: 2 } });
  });

  it("names each field that is blank, missing or not a number", () => {
    expect(parseMeasurements(fields, { a: "x" })).toEqual({ errors: { a: "Enter a number", b: "Enter a number" } });
  });
});

describe("formatValue", () => {
  it("shows up to six decimals without grouping, and a dash when there is no value", () => {
    expect(formatValue(2978.4)).toBe("2978.4");
    expect(formatValue(-0.13581977784474475)).toBe("-0.13582");
    expect(formatValue(33.443549)).toBe("33.443549");
    expect(formatValue(undefined)).toBe("—");
  });
});
