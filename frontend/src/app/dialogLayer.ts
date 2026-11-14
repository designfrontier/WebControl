export type DialogLayer = "user" | "server";

const FIRST_FIELD = [
  "input:not(:disabled)",
  "select:not(:disabled)",
  "textarea:not(:disabled)",
  "button:not(:disabled)",
].map((selector) => `.dialog-body ${selector}`);

const shown: { dialog: HTMLDialogElement; layer: DialogLayer }[] = [];

/** Focuses the marked control, else the body's first field, else the dialog itself. */
export const focusInitial = (dialog: HTMLDialogElement) =>
  (
    dialog.querySelector<HTMLElement>("[data-autofocus]") ??
    dialog.querySelector<HTMLElement>(FIRST_FIELD.join(", ")) ??
    dialog
  ).focus();

const reshow = (dialog: HTMLDialogElement) => {
  dialog.close();
  dialog.showModal();
  focusInitial(dialog);
};

/**
 * Opens `dialog` in the browser's top layer. The top layer stacks in opening order, so a server dialog that is
 * already open is reopened after any user dialog to keep it on top.
 */
export const showDialog = (dialog: HTMLDialogElement, layer: DialogLayer) => {
  dialog.showModal();
  focusInitial(dialog);
  shown.push({ dialog, layer });
  if (layer === "user") shown.filter((entry) => entry.layer === "server").forEach((entry) => reshow(entry.dialog));
};

export const hideDialog = (dialog: HTMLDialogElement) => {
  const index = shown.findIndex((entry) => entry.dialog === dialog);
  if (index >= 0) shown.splice(index, 1);
  if (dialog.open) dialog.close();
};
