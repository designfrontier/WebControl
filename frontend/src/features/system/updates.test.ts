import { describe, expect, it } from "vitest";
import { updates } from "../../test/systemFixtures";
import { explanation, headline, safeUrl, updateCommand, when } from "./updates";

describe("updates", () => {
  it.each([
    ["designfrontier/WebControl", "updateAvailable", "webcontrol-update"],
    ["designfrontier/WebControl", "devBuild", "webcontrol-update --force"],
    ["me/fork", "upToDate", "webcontrol-update --repo me/fork"],
  ] as const)("runs webcontrol-update for %s when %s", (repo, status, command) => {
    expect(updateCommand({ repo, status })).toBe(command);
  });

  it("names an available update even without its release", () => {
    expect(headline(updates({ latest: null }))).toEqual({ text: "Update available: ", tone: "warn" });
  });

  it("explains nothing without a release, and leaves out a release commit GitHub did not give", () => {
    expect(explanation(updates({ latest: null }))).toBe("");
    const release = updates().latest!;
    expect(explanation(updates({ status: "devBuild", latest: { ...release, commit: null } }))).toContain(
      "not the v0.4.0 release. The command",
    );
  });

  it("leaves text that is not a date as it is", () => {
    expect(when("yesterday")).toBe("yesterday");
    expect(when("2026-11-16T18:00:00Z")).toMatch(/2026/);
  });

  it("only keeps https links", () => {
    expect(safeUrl("https://github.com/x")).toBe("https://github.com/x");
    expect(safeUrl("http://github.com/x")).toBeUndefined();
    expect(safeUrl("javascript:alert(1)")).toBeUndefined();
  });
});
