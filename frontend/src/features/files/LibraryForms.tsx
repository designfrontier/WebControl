import { useState } from "react";
import type { FormEvent } from "react";
import { CloseButton } from "../../app/CloseButton";
import type { CreatedDirectory, Library, LibraryFile } from "../../protocol";
import { useSubmit } from "./hooks";
import { fileNameProblem, filesIn, initialDirectory, initialFile } from "./library";
import type { Listing } from "./library";
import { FormError, LibraryPicker, NewDirectoryForm } from "./ui";

interface OpenProps {
  library: Library;
  legend: string;
  open: (file: LibraryFile) => Promise<void>;
  /** Why opening is refused right now, such as a job holding the gcode. */
  lockedReason?: string;
}

/** Choose a file from a library (gcode or boards) and open it. */
export const OpenFromLibrary = ({ library, legend, open, lockedReason }: OpenProps) => {
  const [directory, setDirectory] = useState(() => initialDirectory(library));
  const [selected, setSelected] = useState(() => initialFile(library, initialDirectory(library)));
  const { busy, error, setError, run } = useSubmit();

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!selected) return setError("Choose a file to open.");
    return run(() => open({ directory, name: selected }), `Could not open ${selected}.`);
  };

  return (
    <form onSubmit={submit} noValidate>
      <LibraryPicker
        listing={library}
        directory={directory}
        onDirectoryChange={(next) => {
          setDirectory(next);
          setSelected(null);
        }}
        selected={selected}
        onSelect={setSelected}
        legend={legend}
      />
      <FormError message={error} />
      {lockedReason && <p className="muted">{lockedReason}</p>}
      <div className="actions">
        <button type="submit" className="primary" disabled={busy || lockedReason !== undefined}>
          {busy ? "Opening…" : "Open"}
        </button>
        <CloseButton>Cancel</CloseButton>
      </div>
    </form>
  );
};

interface SaveProps {
  library: Library;
  legend: string;
  save: (directory: string, fileName: string) => Promise<void>;
  createDirectory: (name: string) => Promise<CreatedDirectory>;
}

/** Choose a directory and file name (picking an existing file fills its name in) and save there. */
export const SaveToLibrary = ({ library, legend, save, createDirectory }: SaveProps) => {
  const [listing, setListing] = useState<Listing>(library);
  const [directory, setDirectory] = useState(() => initialDirectory(library));
  const [fileName, setFileName] = useState(library.lastFile);
  const { busy, error, setError, run } = useSubmit();
  const name = fileName.trim();
  const replaces = filesIn(listing, directory).some((file) => file.name === name);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const problem = fileNameProblem(name);
    if (problem) return setError(problem);
    return run(() => save(directory, name), `Could not save ${name}.`);
  };

  const created = ({ directory: next, ...rest }: CreatedDirectory) => {
    setListing(rest);
    setDirectory(next);
  };

  return (
    <>
      <form onSubmit={submit} noValidate>
        <LibraryPicker
          listing={listing}
          directory={directory}
          onDirectoryChange={setDirectory}
          selected={replaces ? name : null}
          onSelect={setFileName}
          legend={legend}
        />
        <div className="field">
          <label htmlFor="save-file-name">File name</label>
          <input
            id="save-file-name"
            value={fileName}
            onChange={(event) => setFileName(event.target.value)}
            autoComplete="off"
            aria-describedby="save-file-name-hint"
          />
          <span id="save-file-name-hint" className={replaces ? "warning" : "muted"}>
            {replaces ? `Saving replaces the existing ${name}.` : "Pick an existing file above to replace it."}
          </span>
        </div>
        <FormError message={error} />
        <div className="actions">
          <button type="submit" className="primary" disabled={busy}>
            {busy ? "Saving…" : "Save"}
          </button>
          <CloseButton>Cancel</CloseButton>
        </div>
      </form>
      <NewDirectoryForm create={createDirectory} onCreated={created} />
    </>
  );
};
