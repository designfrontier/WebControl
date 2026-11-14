import type { ModalMessage } from "../../protocol";
import { unwrapMessage } from "./controls";

export type DialogKind = "alert" | "notification";

export interface ServerDialog {
  id: number;
  kind: DialogKind;
  title: string;
  message: string;
  /** Offers Resume (resumeRun), sent for tool changes and manual Z adjustments. */
  resume: boolean;
  /** Offers Clear alert (stopRun). */
  clear: boolean;
  progress: "none" | "bar" | "spinner";
  /** How many identical copies arrived while this one was open. */
  count: number;
}

export interface DialogState {
  dialogs: ServerDialog[];
  nextId: number;
}

export type DialogEvent =
  | { type: "open"; modal: ModalMessage }
  | { type: "close"; kind: DialogKind; title: string }
  | { type: "dismiss"; id: number };

export const initialDialogState: DialogState = { dialogs: [], nextId: 1 };

const progressOf = (progress: ModalMessage["progress"]): ServerDialog["progress"] =>
  progress === "spinner" ? "spinner" : progress === "true" || progress === true ? "bar" : "none";

const toDialog = (modal: ModalMessage, id: number): ServerDialog | null => {
  // Content modals answer the old UI's requestPage with server-rendered HTML; the new UI never asks for them.
  if (modal.modalType === "content") return null;
  const kind: DialogKind = modal.modalType === "alert" ? "alert" : "notification";
  return {
    id,
    kind,
    title: modal.title,
    message: unwrapMessage(modal.message),
    resume: kind === "notification" && modal.resume === "resume",
    clear: kind === "alert" && modal.resume === "clear",
    progress: kind === "notification" ? progressOf(modal.progress) : "none",
    count: 1,
  };
};

const sameContent = (a: ServerDialog, b: ServerDialog) =>
  a.kind === b.kind && a.title === b.title && a.message === b.message;

/** A queue of server-pushed dialogs. The view shows only the head, so no number of messages can stack overlays. */
export const dialogReducer = (state: DialogState, event: DialogEvent): DialogState => {
  switch (event.type) {
    case "open": {
      const dialog = toDialog(event.modal, state.nextId);
      if (!dialog) return state;
      const duplicate = state.dialogs.find((open) => sameContent(open, dialog));
      return duplicate
        ? {
            ...state,
            dialogs: state.dialogs.map((open) =>
              open === duplicate ? { ...dialog, id: open.id, count: open.count + 1 } : open,
            ),
          }
        : { dialogs: [...state.dialogs, dialog], nextId: state.nextId + 1 };
    }
    case "close": {
      const dialogs = state.dialogs.filter((open) => open.kind !== event.kind || open.title !== event.title);
      return dialogs.length === state.dialogs.length ? state : { ...state, dialogs };
    }
    case "dismiss": {
      const dialogs = state.dialogs.filter((open) => open.id !== event.id);
      return dialogs.length === state.dialogs.length ? state : { ...state, dialogs };
    }
  }
};
