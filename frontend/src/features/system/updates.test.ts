import { describe, expect, it } from "vitest";
import { updates } from "../../test/systemFixtures";
import { explanation, headline, safeUrl, updateCommand, when } from "./updates";

describe("updates", () => {
  it.each([
    ["designfrontier/WebControl", "updateAvailable", "webcontrol-update"],
    ["designfrontier/WebControl", "devBuild", "webcontrol-update --force"],
    ["designfrontier/WebControl", "ahead", "webcontrol-update --version v0.4.0"],
    ["me/fork", "upToDate", "webcontrol-update --repo me/fork"],
  ] as const)("runs webcontrol-update for %s when %s", (repo, status, command) => {
    expect(updateCommand({ repo, status, latest: updates().latest })).toBe(command);
  });

  it("never names a version to install without a release", () => {
    expect(updateCommand({ repo: "designfrontier/WebControl", status: "ahead", latest: null })).toBe(
      "webcontrol-update",
    );
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

  it("points at the button when this page can update, else at the Pi", () => {
    expect(explanation(updates({ canUpdate: true }))).toMatch(/Update it with the button below\.$/);
    expect(explanation(updates({ canUpdate: false }))).toMatch(/Update it on the Pi as below\.$/);
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
