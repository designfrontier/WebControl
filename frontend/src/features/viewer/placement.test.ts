import { gzip } from "pako";
import { describe, expect, it, vi } from "vitest";
import { fakeFetch } from "../../test/fakeFetch";
import { fitsOnBed, ghostFrom, loadGhost, toolpathBounds } from "./placement";
import { DEFAULT_MACHINE } from "./shapes";

const base64 = (value: unknown) => btoa(String.fromCharCode(...gzip(JSON.stringify(value))));
const cut = {
  type: "line",
  points: [
    [0, 0, 0],
    [2, 1, -0.1],
  ],
  dashed: false,
  command: null,
};

describe("toolpathBounds", () => {
  it("spans every point of every buffer, and is null with none", () => {
    expect(toolpathBounds([new Float32Array([1, 2, 0, -3, 4, 0]), [5, -6, 0]])).toEqual({
      min: { x: -3, y: -6 },
      max: { x: 5, y: 4 },
    });
    expect(toolpathBounds([new Float32Array(), []])).toBeNull();
  });
});

describe("fitsOnBed", () => {
  const bounds = { min: { x: -1, y: -1 }, max: { x: 10, y: 5 } };

  it("checks both corners moved to the home", () => {
    expect(fitsOnBed(DEFAULT_MACHINE, bounds, { x: 0, y: 0 })).toBe(true);
    expect(fitsOnBed(DEFAULT_MACHINE, bounds, { x: 40, y: 0 })).toBe(false);
    expect(fitsOnBed(DEFAULT_MACHINE, bounds, { x: -47.5, y: 0 })).toBe(false);
    expect(fitsOnBed(DEFAULT_MACHINE, null, { x: 500, y: 0 })).toBe(true);
  });
});

describe("ghostFrom", () => {
  it("decodes the cuts and keeps the run's home", () => {
    const ghost = ghostFrom({ gcode: base64([cut]), homeX: 3, homeY: -2 });
    expect(ghost?.home).toEqual({ x: 3, y: -2 });
    expect(Array.from(ghost?.cuts ?? [])).toEqual([0, 0, 0, 2, 1, expect.closeTo(-0.1) as number]);
  });

  it("is null with no run, an empty file or unreadable data", () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    expect(ghostFrom(null)).toBeNull();
    expect(ghostFrom({ gcode: "", homeX: 0, homeY: 0 })).toBeNull();
    expect(ghostFrom({ gcode: "bm90IGd6aXA=", homeX: 0, homeY: 0 })).toBeNull();
  });
});

describe("loadGhost", () => {
  it("fetches the last run, and gives null when the request fails", async () => {
    const fetch = fakeFetch({ "/api/viewer/last-run": { gcode: base64([cut]), homeX: 1, homeY: 1 } });
    expect((await loadGhost(fetch))?.home).toEqual({ x: 1, y: 1 });
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    expect(await loadGhost(fakeFetch())).toBeNull();
    expect(warn).toHaveBeenCalledWith("Could not load the last run", expect.anything());
  });
});
