import { getJson, sendJson } from "../../api/http";
import type { Fetch } from "../../api/http";
import { boardDetailsSchema, createdDirectorySchema, librarySchema } from "../../protocol";
import type { BoardUnits, LibraryFile } from "../../protocol";
import { postForm } from "../files/api";
import { libraryPath } from "../files/library";

export const getBoardLibrary = (fetchImpl: Fetch) => getJson(fetchImpl, "/boards/files", librarySchema);

export const getBoardDetails = (fetchImpl: Fetch) => getJson(fetchImpl, "/boards/current", boardDetailsSchema);

export const createBoardDirectory = (fetchImpl: Fetch, name: string) =>
  sendJson(fetchImpl, "POST", "/boards/directories", { name }, createdDirectorySchema);

export const openBoard = (fetchImpl: Fetch, file: LibraryFile) =>
  postForm(fetchImpl, "/openBoard", { selectedBoard: libraryPath(file) });

export const saveBoard = (fetchImpl: Fetch, directory: string, fileName: string) =>
  postForm(fetchImpl, "/saveBoard", { selectedDirectory: directory, fileName });

export interface BoardFields {
  boardID: string;
  material: string;
  width: string;
  height: string;
  thickness: string;
  centerX: string;
  centerY: string;
}

export const editBoard = (fetchImpl: Fetch, fields: BoardFields, units: BoardUnits) =>
  postForm(fetchImpl, "/editBoard", { ...fields, units });

export interface Trims {
  trimTop: string;
  trimBottom: string;
  trimLeft: string;
  trimRight: string;
}

export const trimBoard = (fetchImpl: Fetch, trims: Trims, units: BoardUnits) =>
  postForm(fetchImpl, "/trimBoard", { ...trims, units });
