// jsdom has <dialog> but not showModal(); this stands in for the top layer, Escape and the async close event.
const modalStack: HTMLDialogElement[] = [];

HTMLDialogElement.prototype.showModal = function showModal(this: HTMLDialogElement) {
  if (this.open) throw new DOMException("The dialog is already open.", "InvalidStateError");
  this.setAttribute("open", "");
  modalStack.push(this);
};

HTMLDialogElement.prototype.close = function close(this: HTMLDialogElement) {
  if (!this.open) return;
  this.removeAttribute("open");
  modalStack.splice(modalStack.indexOf(this), 1);
  queueMicrotask(() => this.dispatchEvent(new Event("close")));
};

document.addEventListener("keydown", (event) => {
  const top = modalStack.at(-1);
  if (event.key !== "Escape" || !top) return;
  if (top.dispatchEvent(new Event("cancel", { cancelable: true }))) top.close();
});

/** The open modal dialogs, bottom to top, as the browser's top layer would stack them. */
export const topLayer = () => [...modalStack];
