import { ApiError, getJson, sendJson } from "../../api/http";
import type { Fetch } from "../../api/http";
import {
  createdDirectorySchema,
  formErrorSchema,
  gcodeLibrarySchema,
  gcodeTextSchema,
  libraryListingSchema,
  loadedGcodeSchema,
  type LibraryFile,
} from "../../protocol";
import { libraryPath } from "./library";

export const getGcodeLibrary = (fetchImpl: Fetch) => getJson(fetchImpl, "/files/gcode", gcodeLibrarySchema);

export const getLoadedGcode = (fetchImpl: Fetch) => getJson(fetchImpl, "/files/gcode/loaded", loadedGcodeSchema);

export const getGcodeText = async (fetchImpl: Fetch) =>
  (await getJson(fetchImpl, "/files/gcode/text", gcodeTextSchema)).text;

export const getCustomGcode = async (fetchImpl: Fetch) =>
  (await getJson(fetchImpl, "/files/gcode/custom", gcodeTextSchema)).text;

export const createGcodeDirectory = (fetchImpl: Fetch, name: string) =>
  sendJson(fetchImpl, "POST", "/files/gcode/directories", { name }, createdDirectorySchema);

/** Rejects with a `FormRouteError`, so a refusal shows the server's reason. */
export const deleteGcode = (fetchImpl: Fetch, path: string) =>
  sendJson(fetchImpl, "POST", "/files/gcode/delete", { path }, libraryListingSchema).catch((caught: unknown) => {
    throw caught instanceof ApiError ? new FormRouteError(caught.status, caught.message, caught.body) : caught;
  });

type FormValue = string | Blob | Blob[];

/** A classic form route's failure; `reason` is the route's own explanation, when it sent a `formErrorSchema` body. */
export class FormRouteError extends ApiError {
  readonly reason: string | undefined;

  constructor(status: number, message: string, body: unknown) {
    super(status, message, body);
    this.name = "FormRouteError";
    this.reason = formErrorSchema.safeParse(body).data?.error;
  }
}

/** Posts to one of the classic form routes, which answer 200 or an error, mostly without a useful body. */
export const postForm = async (fetchImpl: Fetch, path: string, fields: Record<string, FormValue>) => {
  const body = new FormData();
  Object.entries(fields).forEach(([key, value]) =>
    (Array.isArray(value) ? value : [value]).forEach((item) => body.append(key, item)),
  );
  const response = await fetchImpl(path, { method: "POST", body });
  if (response.ok) return;
  const errorBody: unknown = await response.json().catch(() => undefined);
  throw new FormRouteError(response.status, `POST ${path} failed with ${response.status}`, errorBody);
};

export const openGcode = (fetchImpl: Fetch, file: LibraryFile) =>
  postForm(fetchImpl, "/openGCode", { selectedGCode: libraryPath(file) });

export const uploadGcode = (fetchImpl: Fetch, directory: string, files: File[]) =>
  postForm(fetchImpl, "/uploadGCode", { selectedDirectory: directory, "file[]": files });

export const saveGcode = (fetchImpl: Fetch, directory: string, fileName: string) =>
  postForm(fetchImpl, "/saveGCode", { selectedDirectory: directory, fileName });

/** Replaces the loaded gcode in memory; the file on disk is untouched. */
export const editGcode = (fetchImpl: Fetch, gcode: string) => postForm(fetchImpl, "/editGCode", { gcode });

export const sendGcode = (fetchImpl: Fetch, gcode: string) => postForm(fetchImpl, "/sendGCode", { gcode });

export const importKinds = {
  restore: {
    path: "/importRestoreWebControl",
    title: "Restore WebControl",
    description: "Restore settings, gcode and boards from a WebControl backup .zip.",
    accept: ".zip",
  },
  groundcontrol: {
    path: "/importFile",
    title: "Import groundcontrol.ini",
    description: "Copy machine settings from a Ground Control groundcontrol.ini file.",
    accept: ".ini",
  },
  webcontrol: {
    path: "/importFileWCJSON",
    title: "Import webcontrol.json",
    description: "Load settings from another WebControl's webcontrol.json file.",
    accept: ".json",
  },
} as const;
export type ImportKind = keyof typeof importKinds;

export const importFile = (fetchImpl: Fetch, kind: ImportKind, file: File) =>
  postForm(fetchImpl, importKinds[kind].path, { file });
