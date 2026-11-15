import { useState } from "react";
import type { FormEvent } from "react";
import { useParams } from "react-router";
import { useFetch, useMachine } from "../../app/context";
import { selectGcodeLocked } from "../../state/machine";
import { CloseButton } from "../../app/CloseButton";
import { importFile, importKinds } from "./api";
import type { ImportKind } from "./api";
import { useSubmit } from "./hooks";
import { FormError, SubPage } from "./ui";

const isImportKind = (kind: string | undefined): kind is ImportKind =>
  kind !== undefined && Object.hasOwn(importKinds, kind);

const ImportForm = ({ kind }: { kind: ImportKind }) => {
  const fetchImpl = useFetch();
  const { description, accept } = importKinds[kind];
  const [file, setFile] = useState<File | null>(null);
  const [done, setDone] = useState(false);
  const { busy, error, setError, run } = useSubmit();
  const locked = useMachine(selectGcodeLocked);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    setDone(false);
    if (!file) return setError("Choose a file first.");
    return run(async () => {
      await importFile(fetchImpl, kind, file);
      setDone(true);
    }, `Could not use ${file.name}. Check that it is the right kind of file.`);
  };

  return (
    <SubPage>
      <p className="muted">{description}</p>
      <form onSubmit={submit} noValidate>
        <div className="field">
          <label htmlFor="import-file">File</label>
          <input
            id="import-file"
            type="file"
            accept={accept}
            onChange={(event) => {
              setFile(event.target.files?.[0] ?? null);
              setDone(false);
            }}
          />
        </div>
        {locked && <p className="muted">A job is running or paused. Stop the job first to change the settings.</p>}
        <FormError message={error} />
        <p role="status" className="muted">
          {done ? `${file?.name} was imported.` : ""}
        </p>
        <div className="actions">
          <button type="submit" className="primary" disabled={busy || locked}>
            {busy ? "Working…" : kind === "restore" ? "Restore" : "Import"}
          </button>
          <CloseButton>{done ? "Done" : "Cancel"}</CloseButton>
        </div>
      </form>
    </SubPage>
  );
};

export const ImportFile = () => {
  const { kind } = useParams();
  return isImportKind(kind) ? <ImportForm key={kind} kind={kind} /> : <p>There is nothing to import here.</p>;
};
