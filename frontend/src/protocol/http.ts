import { z } from "zod";
import {
  boardDetailsSchema,
  editBoardFormSchema,
  openBoardFormSchema,
  saveBoardFormSchema,
  trimBoardFormSchema,
} from "./boards";
import {
  holeyMeasurementsSchema,
  holeyParametersSchema,
  holeyResultSchema,
  opticalCaptureRequestSchema,
  opticalCenterSchema,
  opticalFitSchema,
  opticalPaperQuerySchema,
  opticalScanRequestSchema,
  opticalScanStartedSchema,
  opticalSettingsSavedSchema,
  opticalSettingsSchema,
  opticalStateSchema,
  opticalTestImageSchema,
  triangularMeasurementsSchema,
  triangularParametersSchema,
  triangularResultSchema,
} from "./calibration";
import { errorBodySchema } from "./core";
import {
  classicErrorSchema,
  classicResultSchema,
  classicStatusSchema,
  createdDirectorySchema,
  directoryRequestSchema,
  gcodeFormSchema,
  gcodeLibrarySchema,
  gcodeTextSchema,
  importFormSchema,
  librarySchema,
  loadedGcodeSchema,
  openGcodeFormSchema,
  saveGcodeFormSchema,
  uploadGcodeFormSchema,
} from "./files";
import { machineSetupSchema, quickConfigureSchema } from "./machine";
import { mainScreenStateSchema, startupStateSchema } from "./main";
import { settingsRejectionSchema, settingsSectionIds, settingsSectionSchema, settingsUpdateSchema } from "./settings";
import {
  helpPageSchema,
  helpQuerySchema,
  pidSettingsSchema,
  pidTestRequestSchema,
  pidTestResponseSchema,
  releasesSchema,
  systemInfoSchema,
} from "./system";
import { viewerSnapshotSchema } from "./viewer";

export type BodyType = "json" | "form" | "text" | "html" | "file";

export interface HttpBody {
  type: BodyType;
  schema?: z.ZodType;
  description?: string;
}

export interface HttpEndpoint {
  method: "GET" | "POST" | "PUT";
  /** `{name}` marks a path parameter; Flask writes it `<name>`. */
  path: string;
  summary: string;
  params?: z.ZodType;
  query?: z.ZodType;
  body?: HttpBody;
  responses: Record<number, HttpBody>;
  /** "api" is the JSON API for this UI; "classic" routes predate it and keep their old shapes. */
  family: "api" | "classic" | "static";
}

const json = (schema: z.ZodType, description?: string): HttpBody => ({ type: "json", schema, description });
const form = (schema: z.ZodType): HttpBody => ({ type: "form", schema });
const error = (description: string) => json(errorBodySchema, description);
const htmlNotFound = { type: "html", description: "Flask's default HTML 404 page" } satisfies HttpBody;
const missingField = {
  type: "html",
  description: "Werkzeug's HTML 400 page when a form field is missing",
} satisfies HttpBody;

const api = (endpoint: Omit<HttpEndpoint, "family">): HttpEndpoint => ({ ...endpoint, family: "api" });
const get = (path: string, summary: string, schema: z.ZodType) =>
  api({ method: "GET", path, summary, responses: { 200: json(schema) } });

const pathOutside = json(
  classicErrorSchema,
  '{"status": 400, "error": "Path is outside the library"}; a missing form field gives Werkzeug\'s HTML 400 instead',
);
const gcodeLocked = json(classicErrorSchema, "A job is running or paused; the HTTP reason phrase repeats the error");
const JOB_LOCKED = "Rust backend only: a job is running or paused, or a tool change waits";
const jobLocked = json(classicErrorSchema, JOB_LOCKED);

interface ClassicOptions {
  /** Refused with 409 while a job runs or a tool change waits. */
  locked?: boolean;
  /** Refused with 409 like `locked`, by the Rust backend only. */
  jobLocked?: boolean;
  /** Takes library paths, so answers 400 with a JSON body for one outside the library. */
  paths?: boolean;
}

const classic = (
  path: string,
  summary: string,
  body: z.ZodType,
  { locked = false, jobLocked: rustLocked = false, paths = false }: ClassicOptions,
  ok: HttpBody,
  failed: HttpBody,
): HttpEndpoint => ({
  method: "POST",
  path,
  summary,
  body: form(body),
  responses: Object.fromEntries(
    [
      [200, ok],
      [400, paths ? pathOutside : missingField],
      [409, locked ? gcodeLocked : rustLocked ? jobLocked : undefined],
      [500, failed],
    ].filter(([, body]) => body),
  ),
  family: "classic",
});

const statusRoute = (path: string, summary: string, body: z.ZodType, options: ClassicOptions = {}) =>
  classic(
    path,
    summary,
    body,
    options,
    json(classicStatusSchema, '{"status": 200}'),
    json(classicStatusSchema, '{"status": 500}'),
  );

const resultRoute = (path: string, summary: string, body: z.ZodType, options: ClassicOptions = {}) =>
  classic(path, summary, body, options, json(classicResultSchema, '"success"'), json(classicResultSchema, '"failed"'));

const download = (path: string, summary: string): HttpEndpoint => ({
  method: "GET",
  path,
  summary,
  query: z.object({ rnd: z.string().optional().describe("Cache buster; ignored") }),
  responses: { 200: { type: "file", description: "application/zip" }, 500: json(classicResultSchema, '"failed"') },
  family: "classic",
});

const OPTICAL_OFF = "Optical calibration is turned off in Settings > WebControl Settings";
const opticalBusy = `${OPTICAL_OFF}; or a job, a scan or another calibration is running`;
const optical = (
  method: HttpEndpoint["method"],
  path: string,
  summary: string,
  responses: Record<number, HttpBody>,
  extra: Partial<HttpEndpoint> = {},
) => api({ method, path: `/api/calibration/optical${path}`, summary, responses, ...extra });
const opticalJson = (schema: z.ZodType) => ({ body: json(schema) });

/** Experimental optical calibration (Rust only); every route answers 409 while the setting is off. */
const opticalEndpoints: HttpEndpoint[] = [
  optical("GET", "", "Optical settings, the scan session and the camera", {
    200: json(opticalStateSchema),
    409: error(OPTICAL_OFF),
  }),
  optical(
    "PUT",
    "/settings",
    "Save the optical calibration settings",
    { 200: json(opticalSettingsSavedSchema), 400: error("Missing or non-numeric field"), 409: error(opticalBusy) },
    opticalJson(opticalSettingsSchema),
  ),
  optical(
    "POST",
    "/scan",
    "Start a scan of the extents, or of one square; progress arrives as opticalScanProgress",
    { 200: json(opticalScanStartedSchema), 400: error("Invalid settings or extents"), 409: error(opticalBusy) },
    opticalJson(opticalScanRequestSchema),
  ),
  optical(
    "POST",
    "/test-image",
    "Capture and annotate one frame without moving",
    { 200: json(opticalTestImageSchema), 409: error(opticalBusy), 422: error("No camera or no frame") },
    opticalJson(opticalCaptureRequestSchema),
  ),
  optical(
    "POST",
    "/find-center",
    "Suggest the optical centre from the square under the camera",
    { 200: json(opticalCenterSchema), 409: error(opticalBusy), 422: error("No camera, no frame or no square") },
    opticalJson(opticalCaptureRequestSchema),
  ),
  optical(
    "POST",
    "/fit",
    "Fit the holey geometry to the measured grid, unsaved",
    { 200: json(opticalFitSchema), 409: error(`${OPTICAL_OFF}; or too few squares measured`), 422: error("No fit") },
    { body: json(z.object({}), "{}") },
  ),
  optical(
    "POST",
    "/accept",
    "Store the last fit's geometry and send it to the controller",
    { 200: json(holeyParametersSchema), 409: error(`${opticalBusy}; or nothing fitted yet`) },
    { body: json(z.object({}), "{}") },
  ),
  optical("GET", "/csv", "The measured grid as Python's CSV", {
    200: { type: "file", description: "text/csv" },
    409: error(`${OPTICAL_OFF}; or nothing measured`),
  }),
  optical(
    "GET",
    "/target.pdf",
    "The printable target, tiled on pages with registration marks",
    { 200: { type: "file", description: "application/pdf" }, 409: error(OPTICAL_OFF) },
    { query: opticalPaperQuerySchema },
  ),
];

/** Every HTTP route the new UI calls, with what it sends and every answer the backend gives. */
export const httpEndpoints: HttpEndpoint[] = [
  get("/api/state", "What a client needs on (re)connect", startupStateSchema),
  get("/api/main", "Run status and current gcode line for the control screen", mainScreenStateSchema),
  get("/api/viewer", "What the viewer draws", viewerSnapshotSchema),
  api({
    method: "GET",
    path: "/api/settings/{sectionId}",
    summary: "One settings section as a form",
    params: z.object({ sectionId: z.enum(settingsSectionIds) }),
    responses: { 200: json(settingsSectionSchema), 404: htmlNotFound },
  }),
  api({
    method: "PUT",
    path: "/api/settings/{sectionId}",
    summary: "Save changed settings in a section",
    params: z.object({ sectionId: z.enum(settingsSectionIds) }),
    body: json(settingsUpdateSchema),
    responses: {
      200: json(settingsSectionSchema, "The section as saved"),
      400: json(z.union([settingsRejectionSchema, errorBodySchema]), "Per-setting errors, or a malformed body"),
      404: htmlNotFound,
      409: error(JOB_LOCKED),
    },
  }),
  get("/api/files/gcode", "The gcode library", gcodeLibrarySchema),
  api({
    method: "POST",
    path: "/api/files/gcode/directories",
    summary: "Create a gcode library directory; also broadcasts updateDirectories",
    body: json(directoryRequestSchema),
    responses: { 201: json(createdDirectorySchema), 400: error("Empty or unsafe name") },
  }),
  get("/api/files/gcode/loaded", "Where the loaded gcode came from", loadedGcodeSchema),
  get("/api/files/gcode/text", "The loaded gcode as text", gcodeTextSchema),
  get("/api/files/gcode/custom", "The gcode last sent with /sendGCode", gcodeTextSchema),
  get("/api/boards/files", "The board library", librarySchema),
  api({
    method: "POST",
    path: "/api/boards/directories",
    summary: "Create a board library directory",
    body: json(directoryRequestSchema),
    responses: { 201: json(createdDirectorySchema), 400: error("Empty or unsafe name") },
  }),
  get("/api/boards/current", "The current board in machine units", boardDetailsSchema),
  get("/api/machine/setup", "Firmware-dependent facts for the setup screens", machineSetupSchema),
  get("/api/machine/quick-configure", "The quick-configure answers", quickConfigureSchema),
  api({
    method: "POST",
    path: "/api/machine/quick-configure",
    summary: "Save the quick-configure answers",
    body: json(quickConfigureSchema),
    responses: {
      200: json(quickConfigureSchema, "The answers as saved"),
      400: error("Invalid field"),
      409: error(JOB_LOCKED),
    },
  }),
  get("/api/calibration/triangular", "Current triangular calibration parameters", triangularParametersSchema),
  api({
    method: "POST",
    path: "/api/calibration/triangular",
    summary: "Calculate triangular calibration from measurements",
    body: json(triangularMeasurementsSchema),
    responses: {
      200: json(triangularResultSchema),
      400: error("Missing or out-of-range measurement"),
      422: error("The calculation did not converge"),
    },
  }),
  api({
    method: "POST",
    path: "/api/calibration/triangular/accept",
    summary: "Store the last triangular result",
    body: json(z.object({}), "{}"),
    responses: { 200: json(triangularParametersSchema), 409: error("Nothing calculated yet") },
  }),
  get("/api/calibration/holey", "Current holey calibration parameters", holeyParametersSchema),
  api({
    method: "POST",
    path: "/api/calibration/holey",
    summary: "Calculate holey calibration from measurements",
    body: json(holeyMeasurementsSchema),
    responses: {
      200: json(holeyResultSchema),
      400: error("Missing or non-numeric measurement"),
      422: error("The calculation failed"),
    },
  }),
  api({
    method: "POST",
    path: "/api/calibration/holey/accept",
    summary: "Store the last holey result",
    body: json(z.object({}), "{}"),
    responses: { 200: json(holeyParametersSchema), 409: error("Nothing calculated yet") },
  }),
  ...opticalEndpoints,
  get("/api/system/info", "Version, platform, firmware and update status", systemInfoSchema),
  get("/api/system/pid", "Stored PID gains", pidSettingsSchema),
  api({
    method: "POST",
    path: "/api/system/pid-test",
    summary: "Start a PID test; results arrive later as updatePIDData",
    body: json(pidTestRequestSchema),
    responses: {
      202: json(pidTestResponseSchema),
      400: error("Invalid kind, motor or number"),
      409: error("A job is running"),
    },
  }),
  get("/api/system/releases", "WebControl releases on GitHub", releasesSchema),
  api({
    method: "GET",
    path: "/api/system/help",
    summary: "A docs/ page as markdown",
    query: helpQuerySchema,
    responses: { 200: json(helpPageSchema), 404: error('"no such help page"') },
  }),
  api({
    method: "GET",
    path: "/api/system/help/assets/{path}",
    summary: "An image under docs/assets",
    params: z.object({ path: z.string().describe("May contain slashes") }),
    responses: { 200: { type: "file", description: "The file, typed by extension" }, 404: htmlNotFound },
  }),
  statusRoute("/openGCode", "Load a gcode file from the library", openGcodeFormSchema, { locked: true, paths: true }),
  statusRoute("/uploadGCode", "Upload gcode files into the library and load the first", uploadGcodeFormSchema, {
    locked: true,
    paths: true,
  }),
  statusRoute("/saveGCode", "Save the loaded gcode into the library", saveGcodeFormSchema, { paths: true }),
  resultRoute("/editGCode", "Replace the loaded gcode in memory", gcodeFormSchema, { locked: true }),
  resultRoute("/sendGCode", "Send gcode lines to the controller now", gcodeFormSchema, { jobLocked: true }),
  statusRoute("/importFile", "Import settings from groundcontrol.ini", importFormSchema, { jobLocked: true }),
  statusRoute("/importFileWCJSON", "Import settings from a webcontrol.json", importFormSchema, { jobLocked: true }),
  statusRoute("/importRestoreWebControl", "Restore a WebControl backup zip", importFormSchema, { jobLocked: true }),
  statusRoute("/openBoard", "Load a board from the library", openBoardFormSchema, { paths: true }),
  statusRoute("/saveBoard", "Save the current board into the library", saveBoardFormSchema, { paths: true }),
  resultRoute("/editBoard", "Change the current board's details", editBoardFormSchema),
  resultRoute("/trimBoard", "Trim the current board", trimBoardFormSchema),
  download("/downloadDiagnostics", "Download a diagnostics zip"),
  download("/backupWebControl", "Download a backup zip"),
  {
    method: "GET",
    path: "/static/{filename}",
    summary: "Images the screens show, e.g. /static/images/GPIO/RPI_pin_layout.svg",
    params: z.object({ filename: z.string().describe("May contain slashes") }),
    responses: { 200: { type: "file", description: "The file, typed by extension" }, 404: htmlNotFound },
    family: "static",
  },
  {
    method: "GET",
    path: "/{path}",
    summary: "This UI's built files, with index.html for any other path outside /api/; /next/... redirects here (308)",
    params: z.object({ path: z.string().describe("May be empty or contain slashes") }),
    responses: {
      200: { type: "file", description: "A file from frontend/dist, or index.html with Cache-Control: no-cache" },
      404: { type: "text", description: '"Not found" for a missing path whose last segment has a dot' },
      503: { type: "text", description: "frontend/dist has not been built" },
    },
    family: "static",
  },
];
