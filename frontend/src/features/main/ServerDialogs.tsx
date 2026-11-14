import { useEffect, useReducer, useRef } from "react";
import type { KeyboardEvent } from "react";
import { useClient, useServerMessage } from "../../app/context";
import { dialogReducer, initialDialogState } from "./dialogs";
import type { ServerDialog } from "./dialogs";

/** Hosts the alerts and notifications the server pushes with activateModal; closing never round-trips. */
export const ServerDialogs = () => {
  const client = useClient();
  const [{ dialogs }, dispatch] = useReducer(dialogReducer, initialDialogState);
  useServerMessage("activateModal", (modal) => dispatch({ type: "open", modal }));
  useServerMessage("closeModals", ({ title }) => dispatch({ type: "close", kind: "notification", title }));
  useServerMessage("closeAlertModals", ({ title }) => dispatch({ type: "close", kind: "alert", title }));

  const current = dialogs[0];
  const dismiss = (dialog: ServerDialog) => dispatch({ type: "dismiss", id: dialog.id });
  const act = (dialog: ServerDialog, command: "resumeRun" | "stopRun") => {
    client.sendAction(command);
    dismiss(dialog);
  };

  return current ? (
    <DialogView
      dialog={current}
      pending={dialogs.length - 1}
      onClose={() => dismiss(current)}
      onResume={() => act(current, "resumeRun")}
      onClear={() => act(current, "stopRun")}
    />
  ) : null;
};

interface ViewProps {
  dialog: ServerDialog;
  pending: number;
  onClose: () => void;
  onResume: () => void;
  onClear: () => void;
}

const DialogView = ({ dialog, pending, onClose, onResume, onClear }: ViewProps) => {
  const firstButton = useRef<HTMLButtonElement>(null);
  const opener = useRef<Element | null>(null);

  useEffect(() => {
    opener.current = document.activeElement;
    return () => {
      if (opener.current instanceof HTMLElement && opener.current.isConnected) opener.current.focus();
    };
  }, []);

  useEffect(() => {
    firstButton.current?.focus();
  }, [dialog.id]);

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape") onClose();
  };

  const titleId = `server-dialog-${dialog.id}-title`;
  const bodyId = `server-dialog-${dialog.id}-body`;
  const primary = dialog.resume ? "resume" : dialog.clear ? "clear" : "close";

  return (
    <div className="dialog-backdrop" data-testid="dialog-backdrop">
      <div
        className="server-dialog"
        role={dialog.kind === "alert" ? "alertdialog" : "dialog"}
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={bodyId}
        data-kind={dialog.kind}
        onKeyDown={onKeyDown}
      >
        <header>
          <h2 id={titleId}>{dialog.title.replace(/:$/, "")}</h2>
          {pending > 0 && <p className="muted">{pending} more waiting</p>}
        </header>
        <p id={bodyId} className="server-dialog-message">
          {dialog.message}
          {dialog.count > 1 && <span className="muted"> (×{dialog.count})</span>}
        </p>
        {dialog.progress === "spinner" && <p className="spinner" role="status" aria-label="Working" />}
        {dialog.progress === "bar" && <progress aria-label="Progress" />}
        <footer>
          {dialog.resume && (
            <button type="button" className="primary" ref={firstButton} onClick={onResume}>
              Resume
            </button>
          )}
          {dialog.clear && (
            <button type="button" className="primary" ref={firstButton} onClick={onClear}>
              Clear alert
            </button>
          )}
          <button type="button" ref={primary === "close" ? firstButton : undefined} onClick={onClose}>
            Close
          </button>
        </footer>
      </div>
    </div>
  );
};
