import { useEffectEvent, useId, useLayoutEffect, useRef } from "react";
import type { ReactNode } from "react";
import { hideDialog, showDialog } from "./dialogLayer";
import type { DialogLayer } from "./dialogLayer";

export interface DialogProps {
  title: string;
  onClose: () => void;
  children: ReactNode;
  /** Footer buttons. */
  actions?: ReactNode;
  /** Extra header controls, shown before Close. */
  header?: ReactNode;
  /** Server dialogs always stack above user dialogs. */
  layer?: DialogLayer;
  role?: "dialog" | "alertdialog";
  size?: "small" | "medium" | "large";
  /** A header Close button; dialogs whose footer already closes them can leave it out. */
  closeButton?: boolean;
  closeOnBackdrop?: boolean;
  describedBy?: string;
  kind?: string;
}

/**
 * The one modal: a native <dialog> opened with showModal(), so the page behind is inert and focus stays inside.
 * Escape and Close call `onClose`; the parent decides whether it unmounts.
 */
export const Dialog = ({
  title,
  onClose,
  children,
  actions,
  header,
  layer = "user",
  role = "dialog",
  size = "small",
  closeButton = true,
  closeOnBackdrop = false,
  describedBy,
  kind,
}: DialogProps) => {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  const close = useEffectEvent(onClose);

  useLayoutEffect(() => {
    const dialog = ref.current;
    if (!dialog) return;
    // Read before showModal moves focus into the dialog.
    const opener = document.activeElement;
    const onCancel = (event: Event) => {
      event.preventDefault();
      close();
    };
    // Browsers may close a dialog on a repeated Escape without a cancellable event; reopening for stacking does not.
    const onNativeClose = () => {
      if (!dialog.open) close();
    };
    dialog.addEventListener("cancel", onCancel);
    dialog.addEventListener("close", onNativeClose);
    showDialog(dialog, layer);
    return () => {
      dialog.removeEventListener("cancel", onCancel);
      dialog.removeEventListener("close", onNativeClose);
      hideDialog(dialog);
      if (opener instanceof HTMLElement && opener.isConnected) opener.focus();
    };
  }, [layer]);

  return (
    <dialog
      ref={ref}
      className="dialog"
      data-size={size}
      data-kind={kind}
      role={role === "alertdialog" ? "alertdialog" : undefined}
      aria-labelledby={titleId}
      aria-describedby={describedBy}
      tabIndex={-1}
      onClick={closeOnBackdrop ? (event) => event.target === event.currentTarget && onClose() : undefined}
    >
      <div className="dialog-frame">
        <header className="dialog-header">
          <h2 id={titleId}>{title}</h2>
          {header}
          {closeButton && (
            <button type="button" className="dialog-close" onClick={onClose}>
              Close
            </button>
          )}
        </header>
        <div className="dialog-body">{children}</div>
        {actions && <footer className="dialog-actions">{actions}</footer>}
      </div>
    </dialog>
  );
};

interface ConfirmProps {
  title: string;
  confirmLabel: string;
  danger?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
  children: ReactNode;
}

export const ConfirmDialog = ({ title, confirmLabel, danger = false, onConfirm, onCancel, children }: ConfirmProps) => (
  <Dialog
    title={title}
    onClose={onCancel}
    closeButton={false}
    actions={
      <>
        <button type="button" data-autofocus onClick={onCancel}>
          Cancel
        </button>
        <button type="button" className={danger ? "button-danger" : "button-primary"} onClick={onConfirm}>
          {confirmLabel}
        </button>
      </>
    }
  >
    {children}
  </Dialog>
);
