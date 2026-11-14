import { vi } from "vitest";
import type { BoardDetails, GcodeLibrary, Library } from "../protocol";
import { jsonResponse } from "./fakeFetch";

/** Fixtures and fetch helpers for the Files and Boards screens. */
export const gcodeLibrary = (over: Partial<GcodeLibrary> = {}): GcodeLibrary => ({
  directories: [".", "jobs", "empty"],
  files: [
    { directory: ".", name: "part.nc" },
    { directory: "jobs", name: "arm.nc" },
    { directory: "jobs", name: "leg.nc" },
  ],
  lastDirectory: "jobs",
  lastFile: "leg.nc",
  validExtensions: ".nc, .gcode",
  ...over,
});

export const boardLibrary = (over: Partial<Library> = {}): Library => ({
  directories: [".", "shop"],
  files: [
    { directory: ".", name: "shelf.brd" },
    { directory: "shop", name: "bench.brd" },
  ],
  lastDirectory: ".",
  lastFile: "shelf.brd",
  ...over,
});

export const boardDetails = (over: Partial<BoardDetails> = {}): BoardDetails => ({
  boardID: "Shelf",
  material: "Plywood",
  width: 48,
  height: 24,
  thickness: 0.75,
  centerX: 0,
  centerY: 0,
  units: "inches",
  fileName: "shelf.brd",
  router: { x: 10, y: 5 },
  ...over,
});

/** The fields a request posted as FormData, with repeated keys (like "file[]") as arrays. */
export const postedForm = (init: RequestInit | undefined) => {
  const body = init?.body as FormData;
  return Object.fromEntries(
    [...new Set(body.keys())].map((key) => {
      const values = body.getAll(key);
      return [key, values.length > 1 ? values : values[0]];
    }),
  );
};

/** A responder that records each request body and answers with `status`. */
export const recorder = (status = 200, body: unknown = { status }) => {
  const calls: (RequestInit | undefined)[] = [];
  const respond = vi.fn((init: RequestInit | undefined) => {
    calls.push(init);
    return jsonResponse(body, status);
  });
  return { respond, calls, forms: () => calls.map(postedForm) };
};

/** A responder that never answers, to observe busy states. */
export const pending = () => new Promise<Response>(() => {});
