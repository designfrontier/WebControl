import { useState } from "react";
import type { FormEvent } from "react";
import { useClient, useFetch, useMachine } from "../../app/context";
import { CloseButton } from "../../app/CloseButton";
import { useCloseModal } from "../../app/modalHistory";
import type { CreatedDirectory, GcodeLibrary } from "../../protocol";
import { createGcodeDirectory, getGcodeLibrary, uploadGcode } from "./api";
import { GCODE_LOAD, useResource, useSubmit } from "./hooks";
import { initialDirectory } from "./library";
import { selectGcodeLocked } from "../../state/machine";
import {
  DirectorySelect,
  FormError,
  GCODE_LOCKED_NOTE,
  GcodeLoadingOverlay,
  Loaded,
  NewDirectoryForm,
  SubPage,
} from "./ui";

const UploadForm = ({ library }: { library: GcodeLibrary }) => {
  const fetchImpl = useFetch();
  const client = useClient();
  const close = useCloseModal();
  const [directories, setDirectories] = useState(library.directories);
  const [directory, setDirectory] = useState(() => initialDirectory(library));
  const [files, setFiles] = useState<File[]>([]);
  const { busy, error, setError, run } = useSubmit(GCODE_LOAD);
  const locked = useMachine(selectGcodeLocked);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (files.length === 0) return setError("Choose at least one gcode file to upload.");
    return run(async (signal) => {
      await uploadGcode(fetchImpl, directory, files, signal);
      client.emit("checkForGCodeUpdate", { data: "Please" });
      close();
    }, "The upload failed, or the first file could not be opened as gcode.");
  };

  const created = ({ directory: name, directories: next }: CreatedDirectory) => {
    setDirectories(next);
    setDirectory(name);
  };

  return (
    <>
      <form onSubmit={submit} noValidate>
        <DirectorySelect directories={directories} value={directory} onChange={setDirectory} />
        <div className="field">
          <label htmlFor="upload-files">Gcode files</label>
          <input
            id="upload-files"
            type="file"
            multiple
            accept={library.validExtensions}
            onChange={(event) => setFiles([...(event.target.files ?? [])])}
            aria-describedby="upload-files-hint"
          />
          <span id="upload-files-hint" className="muted">
            {files.length > 1
              ? `${files.length} files selected. All are uploaded and ${files[0].name} is opened.`
              : "The file is uploaded to the directory above and opened."}
          </span>
        </div>
        <FormError message={error} />
        {locked && <p className="muted">{GCODE_LOCKED_NOTE}</p>}
        {busy && <GcodeLoadingOverlay title="Uploading gcode…" />}
        <div className="actions">
          <button type="submit" className="primary" disabled={busy || locked}>
            {busy ? "Uploading…" : "Upload and open"}
          </button>
          <CloseButton>Cancel</CloseButton>
        </div>
      </form>
      <NewDirectoryForm create={(name) => createGcodeDirectory(fetchImpl, name)} onCreated={created} />
    </>
  );
};

export const UploadGcode = () => {
  const { resource } = useResource(getGcodeLibrary);
  return (
    <SubPage>
      <Loaded resource={resource} what="the gcode library">
        {(library) => <UploadForm library={library} />}
      </Loaded>
    </SubPage>
  );
};
