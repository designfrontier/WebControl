import { z } from "zod";
import { binaryField } from "./core";
import type { EventSchemas, MessageSchemas } from "./core";

/** Server -> client commands for opening, uploading, saving and editing gcode. */
export const filesMessages = {
  /** Broadcast after a directory is created in the gcode library. */
  updateDirectories: z.object({ directory: z.string() }),
} satisfies MessageSchemas;

export const filesActions = ["createDirectory", "clearGCode"] as const;

export const filesEvents = {} satisfies EventSchemas;

/** The library's top-level directory; files in subdirectories carry the subdirectory's name. */
export const LIBRARY_ROOT = ".";

export const libraryFileSchema = z.object({ directory: z.string(), name: z.string() });
export type LibraryFile = z.infer<typeof libraryFileSchema>;

const listingShape = { directories: z.array(z.string()), files: z.array(libraryFileSchema) };

/** GET /api/files/gcode and /api/boards/files. */
export const librarySchema = z.object({ ...listingShape, lastDirectory: z.string(), lastFile: z.string() });
export type Library = z.infer<typeof librarySchema>;

export const gcodeLibrarySchema = librarySchema.extend({ validExtensions: z.string() });
export type GcodeLibrary = z.infer<typeof gcodeLibrarySchema>;

/** POST /api/files/gcode/directories and /api/boards/directories: the sanitised name and the refreshed listing. */
export const createdDirectorySchema = z.object({ ...listingShape, directory: z.string() });
export type CreatedDirectory = z.infer<typeof createdDirectorySchema>;

/** GET /api/files/gcode/loaded: directory is null when the file is outside the library. */
export const loadedGcodeSchema = z.object({
  directory: z.string().nullable(),
  name: z.string().nullable(),
  lineCount: z.number(),
});
export type LoadedGcode = z.infer<typeof loadedGcodeSchema>;

/** Error body of a classic form route that explains its refusal, e.g. 409 from /openGCode during a job. */
export const formErrorSchema = z.object({ error: z.string() });

/** GET /api/files/gcode/text and /api/files/gcode/custom. */
export const gcodeTextSchema = z.object({ text: z.string() });

/** POST /api/files/gcode/directories and /api/boards/directories body. */
export const directoryRequestSchema = z.object({ name: z.string() });

export const libraryPathField = z.string().describe('"<directory>/<file name>"; directory "." is the library root.');

/** Form bodies (multipart/form-data) of the classic gcode and import routes. */
export const openGcodeFormSchema = z.object({ selectedGCode: libraryPathField });
export const uploadGcodeFormSchema = z.object({ selectedDirectory: z.string(), "file[]": z.array(binaryField).min(1) });
export const saveGcodeFormSchema = z.object({ selectedDirectory: z.string(), fileName: z.string() });
export const gcodeFormSchema = z.object({ gcode: z.string() });
export const importFormSchema = z.object({ file: binaryField });

/** What most classic form routes answer, with the HTTP status repeated in the body. */
export const classicStatusSchema = z.object({ status: z.number().int() });
/** A classic route's refusal: 409 while a job runs, 400 for a path outside the library. */
export const classicErrorSchema = z.object({ status: z.number().int(), error: z.string() });
/** /editGCode, /sendGCode, /editBoard and /trimBoard answer a bare JSON string. */
export const classicResultSchema = z.enum(["success", "failed"]);
