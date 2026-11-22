import { describe, expect, it } from "vitest";
import { ApiError } from "../../api/http";
import { jobQueue, queueItem as item } from "../../test/queueFixtures";
import { baseName, betweenText, continueLabel, nextText, refusal, statusText } from "./queue";

describe("queue text", () => {
  it("says what happens before each file", () => {
    expect(betweenText(item())).toBe("same tool — starts immediately");
    expect(betweenText(item({ between: "toolChange", tool: 2 }))).toBe("Tool 2 — will pause for a bit change");
    const library = [
      { number: 2, description: "6 mm flat end mill", diameter: 6, units: "mm" as const, flutes: 2, notes: "" },
    ];
    expect(betweenText(item({ between: "toolChange", tool: 2 }), library)).toBe(
      "Tool 2 (6 mm flat end mill) — will pause for a bit change",
    );
    expect(betweenText(item({ between: "noTool", tool: null }))).toBe("no tool named — starts immediately");
    expect(betweenText(item({ between: "unknownTool", tool: 2 }))).toBe(
      "Tool 2, bit in the spindle unknown — starts immediately",
    );
    expect(betweenText(item({ between: "toolChangeSkipped", tool: 2 }))).toBe("Tool 2 — bit change skipped (override)");
    expect(baseName("jobs/leg.nc")).toBe("leg.nc");
    expect(baseName("leg.nc")).toBe("leg.nc");
  });

  it("names the next file after the running one, or the first when none runs", () => {
    const items = [item(), item({ id: 2, file: "shelf.nc" })];
    expect(nextText(jobQueue({ items, headLocked: true }))).toBe("Next: shelf.nc · same tool — starts immediately");
    expect(nextText(jobQueue({ items }))).toBe("First: leg.nc · same tool — starts immediately");
    expect(nextText(jobQueue({ items: [item()], headLocked: true }))).toBeNull();
  });

  it("says where the queue stands", () => {
    const items = [item()];
    expect(statusText(jobQueue({ state: "stopped", reason: "Stop was pressed." }))).toBe(
      "Queue stopped: Stop was pressed.",
    );
    expect(statusText(jobQueue({ state: "stopped" }))).toBe("Queue stopped: no reason given");
    expect(statusText(jobQueue({ items, step: "finishing" }))).toBe("Finishing leg.nc…");
    expect(statusText(jobQueue({ items, step: "loading" }))).toBe("Loading leg.nc…");
    expect(statusText(jobQueue({ items, step: "raising" }))).toBe("Raising Z before leg.nc…");
    expect(statusText(jobQueue({ items, step: "toolChange" }))).toContain("press Resume");
    expect(statusText(jobQueue())).toBe("The queue is empty.");
    expect(statusText(jobQueue({ items, state: "running" }))).toBe("Running leg.nc.");
    expect(statusText(jobQueue({ items }))).toBe("Waiting: play leg.nc or start the queue.");
    expect(continueLabel(jobQueue({ state: "stopped" }))).toBe("Continue queue");
    expect(continueLabel(jobQueue())).toBe("Start queue");
  });

  it("shows the server's refusal, else a fallback", () => {
    expect(refusal(new ApiError(409, "x", { error: "The queue is empty." }), "No")).toBe("The queue is empty.");
    expect(refusal(new Error("boom"), "No")).toBe("No");
  });
});
