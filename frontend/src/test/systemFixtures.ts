import type { HelpPage, PidSettings, SystemInfo, Updates } from "../protocol";
import { fakeFetch } from "./fakeFetch";

export const systemInfo = (over: Partial<SystemInfo> = {}): SystemInfo => ({
  version: "0.95",
  platform: "RPI",
  controllerFirmwareVersion: 1.26,
  firmware: { stock: "1.26", holey: "51.28", custom: "100.07" },
  canTurnOff: false,
  fourMotor: false,
  firmwareBuild: "1.26",
  firmwareSupportsZaxisLimit: false,
  update: { available: false, version: null },
  ...over,
});

export const pidSettings = (over: Partial<PidSettings> = {}): PidSettings => ({
  velocity: { kp: "5", ki: "0.1", kd: "0" },
  position: { kp: "1300", ki: "0", kd: "34" },
  fourMotor: false,
  ...over,
});

export const latestRelease = (over: Partial<NonNullable<Updates["latest"]>> = {}): NonNullable<Updates["latest"]> => ({
  tag: "v0.4.0",
  name: "webcontrol-rs 0.4.0",
  publishedAt: "2026-11-20T10:00:00Z",
  url: "https://github.com/designfrontier/WebControl/releases/tag/v0.4.0",
  notes: "## Faster previews\n\n* Fixes [#12](https://github.com/x/y/issues/12)\n\n<img src=x onerror=alert(1)>",
  commit: "1234567890abcdef1234567890abcdef12345678",
  ...over,
});

export const updates = (over: Partial<Updates> = {}): Updates => ({
  installed: { version: "0.3.0", commit: "dc9bdca" },
  repo: "designfrontier/WebControl",
  latest: latestRelease(),
  status: "updateAvailable",
  error: null,
  checkedAt: Date.UTC(2026, 10, 20, 12, 0),
  backup: { version: "0.2.0", date: "2026-11-16T18:00:00Z" },
  ...over,
});

export const helpPage = (path: string, markdown: string, title = "Page"): HelpPage => ({ path, title, markdown });

export const HELP_INDEX = helpPage(
  "assets/helpPages.md",
  "[Home](help)\n\n[First Steps](GettingStarted/firstSteps.md)\n\n[Controller](assets/Actions/Controller/index.md)",
  "Help Pages",
);

/** A fetch answering every /api/system endpoint with fixtures, overridable per route. */
export const systemFetch = (routes: Parameters<typeof fakeFetch>[0] = {}) =>
  fakeFetch({
    "/api/system/info": systemInfo(),
    "/api/system/pid": pidSettings(),
    "/api/system/updates": updates(),
    "/api/system/help?path=assets%2FhelpPages.md": HELP_INDEX,
    "/api/system/help?path=index.md": helpPage("index.md", "# WebControl\n\nWelcome.", "Home"),
    ...routes,
  });
