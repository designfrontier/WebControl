import { useEffect, useId, useRef } from "react";
import type { ReactNode } from "react";

interface DialogProps {
  title: string;
  onClose: () => void;
  children: ReactNode;
  /** The dialog's buttons; the first focusable element inside gets focus when it opens. */
  actions: ReactNode;
}

/** A client-owned modal: no server round trip, closes on Escape or a backdrop click, restores focus on close. */
export const Dialog = ({ title, onClose, children, actions }: DialogProps) => {
  const titleId = useId();
  const panel = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const opener = document.activeElement as HTMLElement | null;
    panel.current?.querySelector<HTMLElement>("input, button")?.focus();
    return () => opener?.focus();
  }, []);

  return (
    <div className="dialog-backdrop" onClick={(event) => event.target === event.currentTarget && onClose()}>
      <div
        ref={panel}
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onKeyDown={(event) => event.key === "Escape" && onClose()}
      >
        <h2 id={titleId}>{title}</h2>
        <div className="dialog-body">{children}</div>
        <div className="dialog-actions">{actions}</div>
      </div>
    </div>
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
    actions={
      <>
        <button type="button" onClick={onCancel}>
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
