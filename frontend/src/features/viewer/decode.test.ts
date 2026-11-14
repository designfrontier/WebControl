import { gzip } from "pako";
import { describe, expect, it } from "vitest";
import { decodeCutCells, decodeGcode } from "./decode";
import { fromInches, roundForAction, toInches, unitLabel } from "./units";

const gz = (value: unknown) => gzip(JSON.stringify(value));
const toBase64 = (bytes: Uint8Array) => btoa(String.fromCharCode(...bytes));

const moves = [
  {
    type: "line",
    points: [
      [0, 0, 0],
      [1, 2, -0.1],
    ],
    dashed: false,
    command: null,
    color: null,
    radius: null,
  },
];

describe("decodeGcode", () => {
  it("inflates the ArrayBuffer the socket delivers", () => {
    const bytes = gz(moves);
    const buffer = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
    expect(decodeGcode(buffer)).toEqual([{ type: "line", points: moves[0].points, dashed: false, command: null }]);
  });

  it("inflates Uint8Array and the base64 text /api/viewer sends", () => {
    expect(decodeGcode(gz(moves))).toHaveLength(1);
    expect(decodeGcode(toBase64(gz(moves)))).toHaveLength(1);
  });

  it.each([null, "", new ArrayBuffer(0)])("treats %o as no gcode", (payload) => {
    expect(decodeGcode(payload)).toBeNull();
  });

  it("throws on corrupt or unexpected data", () => {
    expect(() => decodeGcode(new Uint8Array([1, 2, 3]))).toThrow();
    expect(() => decodeGcode(gz([{ type: "line" }]))).toThrow();
  });
});

describe("decodeCutCells", () => {
  it("returns the raw cell list", () => {
    expect(decodeCutCells(gz([true, false, 0]))).toEqual([true, false, 0]);
  });
});

describe("units", () => {
  it("converts machine units to scene inches and back", () => {
    expect(toInches(25.4, "MM")).toBe(1);
    expect(toInches(2, "INCHES")).toBe(2);
    expect(toInches(2, null)).toBe(2);
    expect(fromInches(1, "MM")).toBe(25.4);
    expect(fromInches(3, "INCHES")).toBe(3);
  });

  it("labels units and rounds action arguments to 4 decimals", () => {
    expect(unitLabel("MM")).toBe("mm");
    expect(unitLabel(null)).toBe("in");
    expect(roundForAction(1.234567)).toBe(1.2346);
    expect(roundForAction(-0.00004)).toBe(-0);
  });
});
