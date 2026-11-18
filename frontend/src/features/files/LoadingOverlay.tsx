import { useId, useLayoutEffect, useRef } from "react";
import { createPortal } from "react-dom";
import { hideDialog, showDialog } from "../../app/dialogLayer";
import { StopButton } from "../../app/StopButton";
import "./loadingOverlay.css";

interface Props {
  title: string;
  detail: string;
}

/**
 * A full-page modal <dialog> that blocks the whole page until the caller unmounts it; Escape does nothing.
 * It keeps Stop, as everything behind it is inert.
 */
export const LoadingOverlay = ({ title, detail }: Props) => {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  const detailId = useId();

  useLayoutEffect(() => {
    const dialog = ref.current;
    if (!dialog) return;
    const opener = document.activeElement;
    const refuse = (event: Event) => event.preventDefault();
    // Chrome closes the whole dialog stack on a repeated Escape unless the keydown itself is cancelled.
    // A server dialog stacked above keeps its Escape.
    const swallowEscape = (event: KeyboardEvent) => {
      const within = event.target instanceof Element ? event.target.closest("dialog") : null;
      if (event.key === "Escape" && (!within || within === dialog)) event.preventDefault();
    };
    const reopen = () => {
      if (dialog.open) return;
      hideDialog(dialog);
      showDialog(dialog, "user");
    };
    dialog.addEventListener("cancel", refuse);
    dialog.addEventListener("close", reopen);
    document.addEventListener("keydown", swallowEscape, true);
    showDialog(dialog, "user");
    return () => {
      dialog.removeEventListener("cancel", refuse);
      dialog.removeEventListener("close", reopen);
      document.removeEventListener("keydown", swallowEscape, true);
      hideDialog(dialog);
      if (opener instanceof HTMLElement && opener.isConnected) opener.focus();
    };
  }, []);

  // Outside the route modal, so its `.dialog-body` focus rule and styles do not reach in.
  return createPortal(
    <dialog
      ref={ref}
      className="loading-overlay"
      aria-labelledby={titleId}
      aria-describedby={detailId}
      aria-busy="true"
      tabIndex={-1}
    >
      <div className="loading-overlay-card">
        <span className="spinner" aria-hidden="true" />
        <div role="status">
          <h2 id={titleId}>{title}</h2>
          <p id={detailId}>{detail}</p>
        </div>
        <StopButton label="Stop machine" />
      </div>
    </dialog>,
    document.body,
  );
};
