import { describe, expect, it } from "vitest";
import { z } from "zod";
import { fakeFetch, jsonResponse, startupState } from "../test/fakeFetch";
import { ApiError, getJson, requestJson, sendJson } from "./http";
import { getStartupState } from "./state";

const thing = z.object({ id: z.number() });

describe("getJson", () => {
  it("returns the validated body", async () => {
    const fetch = fakeFetch({ "/api/things/1": { id: 1 } });
    await expect(getJson(fetch, "/things/1", thing)).resolves.toEqual({ id: 1 });
    expect(fetch).toHaveBeenCalledWith("/api/things/1", { headers: { Accept: "application/json" } });
  });

  it("throws an ApiError with the status on a non-2xx response", async () => {
    const error = await getJson(fakeFetch(), "/missing", thing).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(ApiError);
    expect(error).toMatchObject({ status: 404, message: "GET /api/missing failed with 404" });
  });

  it("keeps the JSON error body on the ApiError", async () => {
    const fetch = fakeFetch({ "/api/things/1": () => jsonResponse({ errors: { id: "bad" } }, 400) });
    await expect(getJson(fetch, "/things/1", thing)).rejects.toMatchObject({
      status: 400,
      body: { errors: { id: "bad" } },
    });
  });

  it("leaves the body undefined when the error response is not JSON", async () => {
    const fetch = fakeFetch({ "/api/things/1": () => new Response("oops", { status: 500 }) });
    await expect(getJson(fetch, "/things/1", thing)).rejects.toMatchObject({ status: 500, body: undefined });
  });

  it("throws when the body does not match the schema", async () => {
    const fetch = fakeFetch({ "/api/things/1": { id: "one" } });
    await expect(getJson(fetch, "/things/1", thing)).rejects.toThrow("GET /api/things/1 returned an unexpected shape");
  });
});

describe("sendJson", () => {
  it("sends a JSON body with the method", async () => {
    const fetch = fakeFetch({ "POST /api/things": (init) => jsonResponse({ id: JSON.parse(String(init?.body)).id }) });
    await expect(sendJson(fetch, "POST", "/things", { id: 7 }, thing)).resolves.toEqual({ id: 7 });
    expect(fetch).toHaveBeenCalledWith("/api/things", {
      method: "POST",
      body: '{"id":7}',
      headers: { Accept: "application/json", "Content-Type": "application/json" },
    });
  });

  it("does not fall back to GET routes for other methods", async () => {
    const fetch = fakeFetch({ "/api/things": { id: 1 } });
    await expect(sendJson(fetch, "DELETE", "/things", {}, thing)).rejects.toMatchObject({ status: 404 });
  });
});

describe("requestJson", () => {
  it("keeps the caller's headers alongside its own", async () => {
    const fetch = fakeFetch({ "PUT /api/things": { id: 2 } });
    await requestJson(fetch, "/things", thing, { method: "PUT", body: "{}", headers: { "X-Client": "next" } });
    expect(fetch).toHaveBeenCalledWith("/api/things", {
      method: "PUT",
      body: "{}",
      headers: { Accept: "application/json", "Content-Type": "application/json", "x-client": "next" },
    });
  });
});

describe("getStartupState", () => {
  it("reads /api/state", async () => {
    await expect(getStartupState(fakeFetch())).resolves.toEqual(startupState());
  });
});
