import type { HelpPage, PidSettings, Releases, SystemInfo } from "../protocol";
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

export const releases = (over: Partial<Releases> = {}): Releases => ({
  current: "v0.95",
  latest: "v0.96",
  releases: [
    { tag: "v0.96", title: "Faster previews", body: "* Fixes [#12](https://github.com/x/y/issues/12)" },
    { tag: "v0.95", title: "v0.95", body: "" },
    { tag: "v0.94", title: "Older", body: "" },
  ],
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
    "/api/system/releases": releases(),
    "/api/system/help?path=assets%2FhelpPages.md": HELP_INDEX,
    "/api/system/help?path=index.md": helpPage("index.md", "# WebControl\n\nWelcome.", "Home"),
    ...routes,
  });
