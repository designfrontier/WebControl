import { useReducer } from "react";
import { useClient, useServerMessage } from "../../app/context";
import { Dialog } from "../../app/Dialog";
import { dialogReducer, initialDialogState } from "./dialogs";

/**
 * Server-pushed alerts and notifications, one at a time from a queue, always stacked above any open user modal.
 * Closing never round-trips through the server.
 */
export const ServerDialogs = () => {
  const client = useClient();
  const [{ dialogs }, dispatch] = useReducer(dialogReducer, initialDialogState);
  useServerMessage("activateModal", (modal) => dispatch({ type: "open", modal }));
  useServerMessage("closeModals", ({ title }) => dispatch({ type: "close", kind: "notification", title }));
  useServerMessage("closeAlertModals", ({ title }) => dispatch({ type: "close", kind: "alert", title }));

  const current = dialogs[0];
  if (!current) return null;

  const dismiss = () => dispatch({ type: "dismiss", id: current.id });
  const act = (command: "resumeRun" | "stopRun") => {
    client.sendAction(command);
    dismiss();
  };
  const bodyId = `server-dialog-${current.id}-body`;
  const pending = dialogs.length - 1;

  return (
    <Dialog
      key={current.id}
      layer="server"
      role={current.kind === "alert" ? "alertdialog" : "dialog"}
      kind={current.kind}
      title={current.title.replace(/:$/, "")}
      onClose={dismiss}
      closeButton={false}
      describedBy={bodyId}
      header={pending > 0 && <p className="muted">{pending} more waiting</p>}
      actions={
        <>
          {current.resume && (
            <button type="button" className="primary" data-autofocus onClick={() => act("resumeRun")}>
              Resume
            </button>
          )}
          {current.clear && (
            <button type="button" className="primary" data-autofocus onClick={() => act("stopRun")}>
              Clear alert
            </button>
          )}
          <button type="button" data-autofocus={(!current.resume && !current.clear) || undefined} onClick={dismiss}>
            Close
          </button>
        </>
      }
    >
      <p id={bodyId} className="server-dialog-message">
        {current.message}
        {current.count > 1 && <span className="muted"> (×{current.count})</span>}
      </p>
      {current.progress === "spinner" && <p className="spinner" role="status" aria-label="Working" />}
      {current.progress === "bar" && <progress aria-label="Progress" />}
    </Dialog>
  );
};
