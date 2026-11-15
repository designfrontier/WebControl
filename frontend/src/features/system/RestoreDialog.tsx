import { useState } from "react";
import { useFetch } from "../../app/context";
import { restoreBackup } from "./api";
import { Dialog } from "../../app/Dialog";
import { errorText } from "./useLoad";

interface Props {
  onClose: () => void;
  onRestored: () => void;
}

export const RestoreDialog = ({ onClose, onRestored }: Props) => {
  const fetchImpl = useFetch();
  const [file, setFile] = useState<File | null>(null);
  const [state, setState] = useState<{ busy: boolean; error: string }>({ busy: false, error: "" });

  const restore = () => {
    if (!file) return;
    setState({ busy: true, error: "" });
    restoreBackup(fetchImpl, file).then(onRestored, (error: unknown) =>
      setState({ busy: false, error: errorText(error) }),
    );
  };

  return (
    <Dialog
      title="Restore WebControl"
      onClose={onClose}
      actions={
        <>
          <button type="button" onClick={onClose}>
            Cancel
          </button>
          <button type="button" className="button-danger" disabled={!file || state.busy} onClick={restore}>
            {state.busy ? "Restoring…" : "Restore backup"}
          </button>
        </>
      }
    >
      <p>Choose a backup zip made with “Back up WebControl”. It replaces the current settings and files.</p>
      <label className="field">
        Backup file
        <input type="file" accept=".zip" onChange={(event) => setFile(event.target.files?.[0] ?? null)} />
      </label>
      {state.error && (
        <p className="system-error" role="alert">
          {state.error}
        </p>
      )}
    </Dialog>
  );
};
