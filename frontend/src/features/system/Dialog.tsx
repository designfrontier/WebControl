import { useEffect, useId, useRef, useState } from "react";
import type { ReactNode } from "react";
import { useModal } from "../../app/modal";

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
  const backdrop = useRef<HTMLDivElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  // Read during render: once the page behind goes inert, browsers move focus to <body>.
  const [opener] = useState(() => document.activeElement);
  useModal(backdrop, panel, onClose);

  useEffect(() => {
    panel.current?.querySelector<HTMLElement>("input, button")?.focus();
    return () => {
      if (opener instanceof HTMLElement && opener.isConnected) opener.focus();
    };
  }, [opener]);

  return (
    <div
      ref={backdrop}
      className="dialog-backdrop"
      onClick={(event) => event.target === event.currentTarget && onClose()}
    >
      <div ref={panel} className="dialog" role="dialog" aria-modal="true" aria-labelledby={titleId}>
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
