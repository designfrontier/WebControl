import { useId, useRef, useState } from "react";
import type { FormEvent, ReactNode } from "react";
import type { CreatedDirectory } from "../../protocol";
import { useSubmit } from "./hooks";
import { LoadingOverlay } from "./LoadingOverlay";
import type { Resource } from "./hooks";
import { directoryLabel, filesIn } from "./library";
import type { Listing } from "./library";

/** One file or board task, shown in a modal that supplies the heading and Close. */
export const SubPage = ({ children }: { children: ReactNode }) => <div className="fb">{children}</div>;

export const FormError = ({ message }: { message: string | null }) =>
  message ? (
    <p className="form-error" role="alert">
      {message}
    </p>
  ) : null;

interface LoadedProps<T> {
  resource: Resource<T>;
  what: string;
  children: (data: T) => ReactNode;
}

export const Loaded = <T,>({ resource, what, children }: LoadedProps<T>) =>
  resource.status === "ready" ? (
    children(resource.data)
  ) : resource.status === "loading" ? (
    <p className="muted" role="status">
      Loading {what}…
    </p>
  ) : (
    <p className="form-error" role="alert">
      Could not load {what}. Check that WebControl is running, then reload the page.
    </p>
  );

interface PickerProps {
  listing: Listing;
  directory: string;
  onDirectoryChange: (directory: string) => void;
  selected: string | null;
  onSelect: (name: string) => void;
  legend: string;
}

interface DirectorySelectProps {
  directories: string[];
  value: string;
  onChange: (directory: string) => void;
}

export const DirectorySelect = ({ directories, value, onChange }: DirectorySelectProps) => {
  const id = useId();
  return (
    <div className="field">
      <label htmlFor={id}>Directory</label>
      <select id={id} value={value} onChange={(event) => onChange(event.target.value)}>
        {directories.map((name) => (
          <option key={name} value={name}>
            {directoryLabel(name)}
          </option>
        ))}
      </select>
    </div>
  );
};

/** A directory dropdown and the files in it as radio buttons. */
export const LibraryPicker = ({ listing, directory, onDirectoryChange, selected, onSelect, legend }: PickerProps) => {
  const id = useId();
  const files = filesIn(listing, directory);
  return (
    <div className="picker">
      <DirectorySelect directories={listing.directories} value={directory} onChange={onDirectoryChange} />
      <fieldset className="file-list">
        <legend>{legend}</legend>
        {files.length === 0 ? (
          <p className="muted">No files in this directory.</p>
        ) : (
          files.map(({ name }) => (
            <label key={name} className="file-option">
              <input
                type="radio"
                name={`${id}-file`}
                value={name}
                checked={selected === name}
                onChange={() => onSelect(name)}
              />
              <span>{name}</span>
            </label>
          ))
        )}
      </fieldset>
    </div>
  );
};

interface EditorProps {
  label: string;
  value: string;
  onChange: (value: string) => void;
  hint: string;
}

const countLines = (text: string) => (text === "" ? 0 : text.split("\n").length);

const lineNumbers = (count: number) => Array.from({ length: Math.max(count, 1) }, (_, i) => i + 1).join("\n");

/** A plain monospace textarea: no wrapping, spellcheck or autocorrect, which would mangle gcode. */
export const GcodeEditor = ({ label, value, onChange, hint }: EditorProps) => {
  const id = useId();
  const gutter = useRef<HTMLPreElement>(null);
  const lines = countLines(value);
  return (
    <div className="field">
      <label htmlFor={id}>{label}</label>
      <div className="code-frame">
        <pre className="code-gutter" ref={gutter} aria-hidden="true">
          {lineNumbers(lines)}
        </pre>
        <textarea
          id={id}
          className="code-editor"
          value={value}
          onChange={(event) => onChange(event.target.value)}
          onScroll={(event) => gutter.current && (gutter.current.scrollTop = event.currentTarget.scrollTop)}
          rows={18}
          wrap="off"
          spellCheck={false}
          autoCapitalize="off"
          autoComplete="off"
          autoCorrect="off"
          aria-describedby={`${id}-hint`}
        />
      </div>
      <span id={`${id}-hint`} className="muted">
        {lines} {lines === 1 ? "line" : "lines"}. {hint}
      </span>
    </div>
  );
};

interface NewDirectoryProps {
  create: (name: string) => Promise<CreatedDirectory>;
  onCreated: (created: CreatedDirectory) => void;
}

export const NewDirectoryForm = ({ create, onCreated }: NewDirectoryProps) => {
  const id = useId();
  const [name, setName] = useState("");
  const { busy, error, setError, run } = useSubmit();

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!name.trim()) return setError("Enter a name for the new directory.");
    return run(async () => {
      onCreated(await create(name));
      setName("");
    }, "Could not create that directory. Use letters, numbers, dashes or underscores.");
  };

  return (
    <form className="new-directory" onSubmit={submit} noValidate>
      <div className="field">
        <label htmlFor={id}>New directory</label>
        <input id={id} value={name} onChange={(event) => setName(event.target.value)} autoComplete="off" />
      </div>
      <button type="submit" disabled={busy}>
        Create directory
      </button>
      <FormError message={error} />
    </form>
  );
};

export const GCODE_LOCKED_NOTE = "A job is running or paused. Stop the job first to load other gcode.";

/** The server answers a gcode load only once the preview is built, which takes a while for a large file. */
export const GcodeLoadingOverlay = ({ title }: { title: string }) => (
  <LoadingOverlay title={title} detail="Reading the gcode and building its preview. A large file can take a while." />
);
