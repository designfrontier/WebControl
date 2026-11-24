import { useState } from "react";
import type { FormEvent, ReactNode } from "react";
import { CloseButton } from "../../app/CloseButton";
import { ConfirmDialog } from "../../app/Dialog";
import { LIBRARY_ROOT } from "../../protocol";
import type { CreatedDirectory, Library, LibraryFile } from "../../protocol";
import { useSubmit } from "./hooks";
import type { SubmitOptions } from "./hooks";
import { deletePrompt, fileNameProblem, filesIn, initialDirectory, initialFile, libraryPath } from "./library";
import type { Doomed } from "./library";
import type { Listing } from "./library";
import { FormError, LibraryPicker, NewDirectoryForm } from "./ui";

interface OpenProps {
  library: Library;
  legend: string;
  open: (file: LibraryFile, signal: AbortSignal) => Promise<void>;
  /** Why opening is refused right now, such as a job holding the gcode. */
  lockedReason?: string;
  /** Shown while the open request is in flight. */
  busyNote?: ReactNode;
  submitOptions?: SubmitOptions;
  /** Adds the chosen file to the job queue instead of opening it, resolving to the queue's length. */
  enqueue?: (file: LibraryFile) => Promise<number>;
  /** Shown beside Add to queue, such as the file's bit-change override. */
  enqueueOptions?: ReactNode;
  /** Deletes a file's library path or a directory, resolving to the listing after it. */
  remove?: (path: string) => Promise<Listing>;
}

/** Choose a file from a library (gcode or boards) and open it. */
export const OpenFromLibrary = ({
  library,
  legend,
  open,
  lockedReason,
  busyNote,
  submitOptions,
  enqueue,
  enqueueOptions,
  remove,
}: OpenProps) => {
  const [listing, setListing] = useState<Listing>(library);
  const [directory, setDirectory] = useState(() => initialDirectory(library));
  const [selected, setSelected] = useState(() => initialFile(library, initialDirectory(library)));
  const [queued, setQueued] = useState<string | null>(null);
  const [doomed, setDoomed] = useState<Doomed | null>(null);
  const { busy, error, setError, run } = useSubmit(submitOptions);
  const adding = useSubmit();
  const deleting = useSubmit();

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!selected) return setError("Choose a file to open.");
    return run((signal) => open({ directory, name: selected }, signal), `Could not open ${selected}.`);
  };

  const addToQueue = (add: (file: LibraryFile) => Promise<number>) => {
    setQueued(null);
    if (!selected) return adding.setError("Choose a file to add.");
    return adding.run(async () => {
      const count = await add({ directory, name: selected });
      setQueued(`Added ${selected} to the queue. The queue now has ${count} ${count === 1 ? "file" : "files"}.`);
    }, `Could not add ${selected} to the queue.`);
  };

  const askDeleteFile = () =>
    selected
      ? setDoomed({ path: libraryPath({ directory, name: selected }), label: selected })
      : deleting.setError("Choose a file to delete.");

  const confirmDelete = (doom: (path: string) => Promise<Listing>, { path, label }: Doomed) => {
    setDoomed(null);
    return deleting.run(async () => {
      const next = await doom(path);
      setListing(next);
      setSelected(null);
      if (!next.directories.includes(directory)) setDirectory(LIBRARY_ROOT);
    }, `Could not delete ${label}.`);
  };

  return (
    <form onSubmit={submit} noValidate>
      <LibraryPicker
        listing={listing}
        directory={directory}
        onDirectoryChange={(next) => {
          setDirectory(next);
          setSelected(null);
        }}
        selected={selected}
        onSelect={setSelected}
        legend={legend}
      />
      <FormError message={error ?? adding.error ?? deleting.error} />
      {lockedReason && <p className="muted">{lockedReason}</p>}
      {enqueue && (
        <p className="muted">
          Open loads the file now, ready for Play. Add to queue saves it for later: the queue runs its files one after
          another.
        </p>
      )}
      {enqueue && enqueueOptions}
      {busy && busyNote}
      <div className="actions">
        <button type="submit" className="primary" disabled={busy || lockedReason !== undefined}>
          {busy ? "Opening…" : "Open"}
        </button>
        {enqueue && (
          <button type="button" disabled={busy || adding.busy} onClick={() => addToQueue(enqueue)}>
            {adding.busy ? "Adding…" : "Add to queue"}
          </button>
        )}
        {remove && (
          <button type="button" disabled={busy || deleting.busy} onClick={askDeleteFile}>
            Delete file
          </button>
        )}
        {remove && directory !== LIBRARY_ROOT && (
          <button
            type="button"
            disabled={busy || deleting.busy}
            onClick={() => setDoomed({ path: directory, label: directory, files: filesIn(listing, directory).length })}
          >
            Delete directory
          </button>
        )}
        <CloseButton>Cancel</CloseButton>
      </div>
      {queued && <p role="status">{queued}</p>}
      {remove && doomed && (
        <ConfirmDialog
          title="Delete from the library"
          confirmLabel="Delete"
          danger
          onConfirm={() => confirmDelete(remove, doomed)}
          onCancel={() => setDoomed(null)}
        >
          <p>{deletePrompt(doomed)}</p>
        </ConfirmDialog>
      )}
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
