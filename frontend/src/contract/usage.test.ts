import { describe, expect, it } from "vitest";
import { getStartupState } from "../api/state";
import * as boards from "../features/boards/api";
import * as calibration from "../features/calibration/api";
import * as optical from "../features/calibration/opticalApi";
import { opticalSettings } from "../features/calibration/opticalFixtures";
import * as files from "../features/files/api";
import * as machine from "../features/machine/api";
import { quickConfigureValues } from "../features/machine/fixtures";
import * as settings from "../features/settings/api";
import * as system from "../features/system/api";
import * as tools from "../features/tools/api";
import * as viewer from "../features/viewer/api";
import { httpEndpoints, settingsSectionIds } from "../protocol";
import type { HttpEndpoint } from "../protocol";

interface Call {
  method: string;
  url: string;
  body: unknown;
}

const sources = import.meta.glob<string>(["../**/*.{ts,tsx}", "!../**/*.test.{ts,tsx}", "!../test/**"], {
  query: "?raw",
  import: "default",
  eager: true,
});

const pattern = ({ path }: HttpEndpoint) =>
  new RegExp(`^${path.replace(/\{[^}]+\}/g, "[^?]+").replace(/\//g, "\\/")}(\\?.*)?$`);
/** The UI's catch-all never answers under /api/, so an unlisted API call is not mistaken for a page. */
const endpointFor = (method: string, url: string) =>
  httpEndpoints.find(
    (endpoint) =>
      endpoint.method === method &&
      pattern(endpoint).test(url) &&
      !(endpoint.family === "static" && url.startsWith("/api/")),
  );

const formFields = (body: FormData) =>
  Object.fromEntries(
    [...new Set(body.keys())].map((key) => {
      const values = body.getAll(key).map((value) => (value instanceof Blob ? "<binary>" : value));
      return [key, key.endsWith("[]") ? values : values[0]];
    }),
  );
const decodeBody = (body: unknown) =>
  body instanceof FormData ? formFields(body) : typeof body === "string" ? JSON.parse(body) : undefined;

const file = new File(["G0 X0"], "part.nc");
const libraryFile = { directory: ".", name: "part.nc" };
const pidRequest = {
  kind: "position",
  motor: "L",
  start: "0",
  stop: "100",
  steps: "1",
  version: "2",
  time: "2000",
  kp: "1",
  ki: "0",
  kd: "0",
} as const;

/** Every request the UI's api modules make, recorded against a server that refuses them all. */
const recordUiRequests = async () => {
  const calls: Call[] = [];
  const fetch = async (url: string, init?: RequestInit) => {
    calls.push({ method: init?.method ?? "GET", url, body: decodeBody(init?.body) });
    return new Response("null", { status: 500 });
  };
  const requests: (() => Promise<unknown>)[] = [
    () => getStartupState(fetch),
    () => boards.getBoardLibrary(fetch),
    () => boards.getBoardDetails(fetch),
    () => boards.createBoardDirectory(fetch, "shop"),
    () => boards.openBoard(fetch, { directory: "shop", name: "bench.brd" }),
    () => boards.saveBoard(fetch, "shop", "bench.brd"),
    () =>
      boards.editBoard(
        fetch,
        { boardID: "b", material: "Plywood", width: "48", height: "24", thickness: "0.75", centerX: "0", centerY: "0" },
        "inches",
      ),
    () => boards.trimBoard(fetch, { trimTop: "1", trimBottom: "0", trimLeft: "0", trimRight: "0" }, "mm"),
    () => calibration.getTriangular(fetch),
    () => calibration.calculateTriangular(fetch, { cut12: 1930, cut34: 1930, cut5: 300, bitDiameter: 6.35 }),
    () => calibration.acceptTriangular(fetch),
    () => calibration.getHoley(fetch),
    () =>
      calibration.calculateHoley(
        fetch,
        Array.from({ length: 12 }, () => 100),
      ),
    () => calibration.acceptHoley(fetch),
    () => optical.getOptical(fetch),
    () => optical.saveOpticalSettings(fetch, opticalSettings()),
    () => optical.startOpticalScan(fetch, opticalSettings()),
    () => optical.startOpticalScan(fetch, opticalSettings(), { i: -15, j: 7 }),
    () => optical.captureTestImage(fetch, opticalSettings()),
    () => optical.findOpticalCenter(fetch, opticalSettings()),
    () => optical.fitOpticalGeometry(fetch),
    () => optical.acceptOpticalGeometry(fetch),
    () => files.getGcodeLibrary(fetch),
    () => files.getLoadedGcode(fetch),
    () => files.getGcodeText(fetch),
    () => files.getCustomGcode(fetch),
    () => files.createGcodeDirectory(fetch, "jobs"),
    () => files.openGcode(fetch, libraryFile),
    () => files.uploadGcode(fetch, ".", [file, file]),
    () => files.saveGcode(fetch, ".", "part.nc"),
    () => files.editGcode(fetch, "G0 X1"),
    () => files.sendGcode(fetch, "G0 X1"),
    ...(["restore", "groundcontrol", "webcontrol"] as const).map((kind) => () => files.importFile(fetch, kind, file)),
    () => machine.getMachineSetup(fetch),
    () => machine.getQuickConfigure(fetch),
    () => machine.saveQuickConfigure(fetch, quickConfigureValues()),
    ...settingsSectionIds.map((id) => () => settings.getSettingsSection(fetch, id)),
    () => settings.saveSettingsSection(fetch, "maslow", { bedWidth: "2438.4", enableCustom: true }),
    () => system.getSystemInfo(fetch),
    () => system.getPidSettings(fetch),
    () => system.startPidTest(fetch, pidRequest),
    () => system.getUpdates(fetch),
    () => system.startUpdate(fetch, "update"),
    () => system.startUpdate(fetch, "rollback"),
    () => system.getHelpPage(fetch, "Actions/actions.md"),
    () => system.restoreBackup(fetch, file),
    () => viewer.getViewerSnapshot(fetch),
    () => tools.getToolLibrary(fetch),
    () => tools.getFileTools(fetch),
    () => tools.addTool(fetch, { number: 1, description: '1/4" endmill', diameter: 6.35, units: "mm", flutes: 2 }),
    () => tools.updateTool(fetch, 1, { number: 2, description: "v-bit", notes: "60 deg" }),
    () => tools.deleteTool(fetch, 2),
  ];
  await Promise.all(requests.map((request) => request().catch(() => undefined)));
  const links = [
    system.helpAssetUrl("../assets/boards/board.png"),
    system.downloadUrl("downloadDiagnostics"),
    system.downloadUrl("backupWebControl"),
    optical.opticalCsvUrl,
    optical.opticalTargetUrl("a4"),
  ];
  return [...calls, ...links.map((url) => ({ method: "GET", url, body: undefined }))];
};

/** /api paths passed to getJson/sendJson anywhere outside the feature api modules, e.g. GET /api/main. */
const literalApiPaths = () =>
  Object.values(sources).flatMap((source) =>
    [...source.matchAll(/(getJson|sendJson)\(\s*\w+,\s*(?:"(\w+)",\s*)?["`](\/[^"`]*)["`]/g)].map(
      ([, helper, method, path]) => ({
        method: helper === "getJson" ? "GET" : method!,
        url: `/api${path!.replace(/\$\{[^}]+\}/g, "x")}`,
      }),
    ),
  );

describe("the HTTP contract", () => {
  it("covers every request the UI makes, and each request body fits its schema", async () => {
    const calls = await recordUiRequests();
    expect(literalApiPaths()).toContainEqual({ method: "GET", url: "/api/main" });
    expect(calls.filter(({ body }) => body !== undefined)).toHaveLength(33);
    const unlisted = [...calls, ...literalApiPaths()].filter(({ method, url }) => !endpointFor(method, url));
    expect(unlisted).toEqual([]);

    const misfits = calls.flatMap(({ method, url, body }) => {
      const endpoint = endpointFor(method, url)!;
      const query = Object.fromEntries(new URL(url, "http://ui").searchParams);
      const bodyCheck = endpoint.body?.schema?.safeParse(body);
      const queryCheck = endpoint.query?.safeParse(query);
      return [bodyCheck, queryCheck]
        .filter((check) => check && !check.success)
        .map((check) => ({ url, issues: check!.error!.issues }));
    });
    expect(misfits).toEqual([]);
  });

  it("lists only endpoints the UI calls", async () => {
    const calls = [...(await recordUiRequests()), ...literalApiPaths()];
    const unused = httpEndpoints.filter(
      (endpoint) =>
        endpoint.family !== "static" && !calls.some(({ method, url }) => endpointFor(method, url) === endpoint),
    );
    expect(unused.map(({ method, path }) => `${method} ${path}`)).toEqual([]);
  });

  it("serves the images and SPA routes the static entries describe", () => {
    const text = Object.values(sources).join("\n");
    expect(text).toMatch(/"\/static\/images\//);
    expect(endpointFor("GET", "/static/images/GPIO/RPI_pin_layout.svg")?.family).toBe("static");
    expect(endpointFor("GET", "/files/open")?.family).toBe("static");
    expect(endpointFor("GET", "/api/nope")).toBeUndefined();
  });
});
