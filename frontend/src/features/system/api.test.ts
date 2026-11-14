import { describe, expect, it } from "vitest";
import { fakeFetch, jsonResponse } from "../../test/fakeFetch";
import {
  downloadUrl,
  getHelpPage,
  getPidSettings,
  getReleases,
  getSystemInfo,
  helpAssetUrl,
  restoreBackup,
  startPidTest,
} from "./api";
import { helpPage, pidSettings, releases, systemInfo } from "../../test/systemFixtures";

describe("system api", () => {
  it("reads info, PID settings and releases", async () => {
    const fetch = fakeFetch({
      "/api/system/info": systemInfo(),
      "/api/system/pid": pidSettings(),
      "/api/system/releases": releases(),
    });
    await expect(getSystemInfo(fetch)).resolves.toEqual(systemInfo());
    await expect(getPidSettings(fetch)).resolves.toEqual(pidSettings());
    await expect(getReleases(fetch)).resolves.toEqual(releases());
  });

  it("asks for a help page by its encoded docs path", async () => {
    const page = helpPage("Actions/Controller/index.md", "# Controller");
    const fetch = fakeFetch({ "/api/system/help?path=Actions%2FController%2Findex.md": page });
    await expect(getHelpPage(fetch, "Actions/Controller/index.md")).resolves.toEqual(page);
  });

  it("posts a PID test", async () => {
    const fetch = fakeFetch({ "POST /api/system/pid-test": { status: "sent" } });
    const request = {
      kind: "velocity",
      motor: "L",
      start: "0",
      stop: "1",
      steps: "1",
      version: "1",
      kp: "1",
      ki: "0",
      kd: "0",
    } as const;
    await expect(startPidTest(fetch, request)).resolves.toEqual({ status: "sent" });
    expect(JSON.parse(String(fetch.mock.calls[0][1]?.body))).toEqual(request);
  });

  it("maps doc image paths onto the help asset route", () => {
    expect(helpAssetUrl("../../assets/boards/a b.png")).toBe("/api/system/help/assets/boards/a b.png");
    expect(helpAssetUrl("assets/Screenshot.png")).toBe("/api/system/help/assets/Screenshot.png");
  });

  it("builds cache-busting download links", () => {
    expect(downloadUrl("downloadDiagnostics", 42)).toBe("/downloadDiagnostics?rnd=42");
    expect(downloadUrl("backupWebControl")).toMatch(/^\/backupWebControl\?rnd=\d+$/);
  });
});

describe("restoreBackup", () => {
  const file = new File(["zip"], "wc_backup.zip", { type: "application/zip" });

  it("uploads the file as form data to the classic restore route", async () => {
    const fetch = fakeFetch({ "POST /importRestoreWebControl": { status: 200 } });
    await restoreBackup(fetch, file);
    const body = fetch.mock.calls[0][1]?.body as FormData;
    expect(body.get("file")).toBeInstanceOf(File);
  });

  it.each([
    ["a 500", () => jsonResponse({ status: 500 }, 500)],
    ["a non-JSON body", () => new Response("oops", { status: 200 })],
  ])("rejects on %s", async (_label, respond) => {
    const fetch = fakeFetch({ "POST /importRestoreWebControl": respond });
    await expect(restoreBackup(fetch, file)).rejects.toThrow(/Restore failed with (500|200)/);
  });
});
