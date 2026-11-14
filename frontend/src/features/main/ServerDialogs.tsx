import { useEffect, useEffectEvent, useReducer, useRef, useState } from "react";
import type { ReactNode } from "react";
import { useClient, useServerMessage } from "../../app/context";
import { trapTab } from "../../app/modal";
import { dialogReducer, initialDialogState } from "./dialogs";
import type { ServerDialog } from "./dialogs";

/** Hosts server-pushed alerts over `children`, which go inert while one is open; closing never round-trips. */
export const ServerDialogs = ({ children }: { children: ReactNode }) => {
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

  return (
    <>
      <div className="dialog-host" inert={current !== undefined}>
        {children}
      </div>
      {current && (
        <DialogView
          dialog={current}
          pending={dialogs.length - 1}
          onClose={() => dismiss(current)}
          onResume={() => act(current, "resumeRun")}
          onClear={() => act(current, "stopRun")}
        />
      )}
    </>
  );
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
  const container = useRef<HTMLDivElement>(null);
  // Read during render: once the page behind goes inert, browsers move focus to <body>.
  const [opener] = useState(() => document.activeElement);

  useEffect(
    () => () => {
      if (opener instanceof HTMLElement && opener.isConnected) opener.focus();
    },
    [opener],
  );

  useEffect(() => {
    firstButton.current?.focus();
  }, [dialog.id]);

  const onKeyDown = useEffectEvent((event: KeyboardEvent) => {
    if (event.key === "Escape") onClose();
    if (event.key === "Tab" && container.current) trapTab(event, container.current);
  });

  useEffect(() => {
    const listener = (event: KeyboardEvent) => onKeyDown(event);
    document.addEventListener("keydown", listener);
    return () => document.removeEventListener("keydown", listener);
  }, []);

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
        ref={container}
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
