import { useState } from "react";
import type { FormEvent } from "react";
import { Link, Navigate, useParams } from "react-router";
import { useFetch } from "../../app/context";
import { importFile, importKinds } from "./api";
import type { ImportKind } from "./api";
import { useSubmit } from "./hooks";
import { FormError, SubPage } from "./ui";

const isImportKind = (kind: string | undefined): kind is ImportKind =>
  kind !== undefined && Object.hasOwn(importKinds, kind);

const ImportForm = ({ kind }: { kind: ImportKind }) => {
  const fetchImpl = useFetch();
  const { title, description, accept } = importKinds[kind];
  const [file, setFile] = useState<File | null>(null);
  const [done, setDone] = useState(false);
  const { busy, error, setError, run } = useSubmit();

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
    <SubPage title={title} back="/files">
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
        <FormError message={error} />
        <p role="status" className="muted">
          {done ? `${file?.name} was imported.` : ""}
        </p>
        <div className="actions">
          <button type="submit" className="primary" disabled={busy}>
            {busy ? "Working…" : kind === "restore" ? "Restore" : "Import"}
          </button>
          <Link className="button" to="/files">
            {done ? "Done" : "Cancel"}
          </Link>
        </div>
      </form>
    </SubPage>
  );
};

export const ImportFile = () => {
  const { kind } = useParams();
  return isImportKind(kind) ? <ImportForm key={kind} kind={kind} /> : <Navigate to="/files" replace />;
};
