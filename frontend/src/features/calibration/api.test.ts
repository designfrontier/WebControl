import { describe, expect, it, vi } from "vitest";
import { z } from "zod";
import { fakeFetch, jsonResponse } from "../../test/fakeFetch";
import { calculateHoley, calculateTriangular, getOptical, saveOpticalSettings, submit } from "./api";
import { opticalSettings, opticalState } from "./fixtures";

const schema = z.object({ value: z.number() });

describe("submit", () => {
  it("sends JSON and returns the validated reply", async () => {
    const fetch = fakeFetch({ "POST /api/thing": { value: 3 } });
    await expect(submit(fetch, "POST", "/thing", { a: 1 }, schema)).resolves.toEqual({ ok: true, data: { value: 3 } });
    const [, init] = fetch.mock.calls[0];
    expect(init).toMatchObject({ method: "POST", body: '{"a":1}' });
    expect(new Headers(init?.headers).get("Content-Type")).toBe("application/json");
  });

  it("passes on the server's error message", async () => {
    const fetch = fakeFetch({ "POST /api/thing": () => jsonResponse({ error: "Too long" }, 400) });
    await expect(submit(fetch, "POST", "/thing", {}, schema)).resolves.toEqual({ ok: false, error: "Too long" });
  });

  it("falls back to the status when the error body is not JSON", async () => {
    const fetch = fakeFetch({ "PUT /api/thing": () => new Response("oops", { status: 502 }) });
    await expect(submit(fetch, "PUT", "/thing", {}, schema)).resolves.toEqual({
      ok: false,
      error: "The server answered 502.",
    });
  });

  it("rejects a reply of the wrong shape", async () => {
    const fetch = fakeFetch({ "POST /api/thing": { value: "3" } });
    await expect(submit(fetch, "POST", "/thing", {}, schema)).resolves.toEqual({
      ok: false,
      error: "The server sent an unexpected reply.",
    });
  });

  it("reports a network failure", async () => {
    const fetch = vi.fn(() => Promise.reject(new TypeError("Failed to fetch")));
    await expect(submit(fetch, "POST", "/thing", {}, schema)).resolves.toEqual({
      ok: false,
      error: "Could not reach the server.",
    });
  });
});

describe("calibration endpoints", () => {
  it("posts triangular measurements as an object and holey ones as a list", async () => {
    const fetch = fakeFetch({
      "POST /api/calibration/triangular": {
        motorYoffset: 1,
        rotationRadius: 2,
        chainSagCorrection: 3,
        calibrationError: 0,
      },
      "POST /api/calibration/holey": {
        motorYoffset: 1,
        distanceBetweenMotors: 2,
        leftChainTolerance: 3,
        rightChainTolerance: 4,
        calibrationError: 0,
      },
    });
    await calculateTriangular(fetch, { cut12: 1930 });
    await calculateHoley(fetch, [1, 2]);
    expect(fetch.mock.calls.map(([, init]) => init?.body)).toEqual(['{"cut12":1930}', '{"measurements":[1,2]}']);
  });

  it("loads and saves optical settings", async () => {
    const fetch = fakeFetch({
      "/api/calibration/optical": opticalState(),
      "PUT /api/calibration/optical/settings": { settings: opticalSettings({ scaleX: 2 }) },
    });
    await expect(getOptical(fetch)).resolves.toEqual(opticalState());
    await expect(saveOpticalSettings(fetch, opticalSettings())).resolves.toEqual({
      ok: true,
      data: { settings: opticalSettings({ scaleX: 2 }) },
    });
  });
});
