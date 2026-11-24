import { LIBRARY_ROOT } from "../../protocol";
import type { Library, LibraryFile } from "../../protocol";

export type Listing = Pick<Library, "directories" | "files">;

export const filesIn = (listing: Listing, directory: string) =>
  listing.files.filter((file) => file.directory === directory);

/** The "directory/name" form the classic open routes expect, e.g. "./part.nc" or "jobs/part.nc". */
export const libraryPath = ({ directory, name }: LibraryFile) => `${directory}/${name}`;

export const directoryLabel = (directory: string) => (directory === LIBRARY_ROOT ? "Top level" : directory);

export const initialDirectory = (library: Library) =>
  library.directories.includes(library.lastDirectory) ? library.lastDirectory : LIBRARY_ROOT;

/** The last-used file, if it is in `directory`. */
export const initialFile = (library: Library, directory: string) =>
  filesIn(library, directory).some((file) => file.name === library.lastFile) ? library.lastFile : null;

export const fileNameProblem = (name: string) =>
  !name.trim() ? "Enter a file name." : /[\\/]/.test(name) ? "A file name cannot contain slashes." : null;

export const describeLocation = (directory: string | null, name: string) =>
  directory === null || directory === LIBRARY_ROOT ? name : `${directory}/${name}`;

/** A file or directory about to be deleted; `files` is set for a directory. */
export interface Doomed {
  path: string;
  label: string;
  files?: number;
}

export const deletePrompt = ({ label, files }: Doomed) =>
  files === undefined
    ? `Delete ${label}? This cannot be undone.`
    : files === 0
      ? `Delete the empty directory ${label}? This cannot be undone.`
      : `Delete the directory ${label} and the ${files === 1 ? "file" : `${files} files`} in it? This cannot be undone.`;
