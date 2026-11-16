import { describe, expect, it } from "vitest";
import { decimalPoint } from "./decimal";

describe("decimalPoint", () => {
  it("turns a lone decimal comma into a point", () => {
    expect(decimalPoint("2,5")).toBe("2.5");
    expect(decimalPoint(" -0,25 ")).toBe(" -0.25 ");
    expect(decimalPoint(",5")).toBe(".5");
  });

  it("leaves points, plain integers and blanks alone", () => {
    expect(decimalPoint("2.5")).toBe("2.5");
    expect(decimalPoint("12")).toBe("12");
    expect(decimalPoint("")).toBe("");
  });

  it("leaves ambiguous grouping as typed so it fails to parse", () => {
    ["1,234.5", "1.234,5", "1,2,3"].forEach((text) => {
      expect(decimalPoint(text)).toBe(text);
      expect(Number(decimalPoint(text))).toBeNaN();
    });
  });
});
