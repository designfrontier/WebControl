import { describe, expect, it } from "vitest";
import type { ModalMessage } from "../../protocol";
import { dialogReducer, initialDialogState } from "./dialogs";
import type { DialogEvent, DialogState } from "./dialogs";

const alert = (message: string, over: Partial<ModalMessage> = {}): DialogEvent => ({
  type: "open",
  modal: { title: "Alert", message: JSON.stringify(message), modalType: "alert", resume: "false", ...over },
});

const notification = (message: string, over: Partial<ModalMessage> = {}): DialogEvent => ({
  type: "open",
  modal: { title: "Notification:", message, modalType: "notification", resume: "false", progress: "false", ...over },
});

const run = (...events: DialogEvent[]) => events.reduce<DialogState>(dialogReducer, initialDialogState);

describe("dialogReducer", () => {
  it("queues alerts and notifications in arrival order", () => {
    const state = run(alert("No GCode file loaded."), notification("Tool change: T2", { resume: "resume" }));
    expect(state.dialogs).toEqual([
      expect.objectContaining({ id: 1, kind: "alert", message: "No GCode file loaded.", resume: false }),
      expect.objectContaining({ id: 2, kind: "notification", message: "Tool change: T2", resume: true }),
    ]);
  });

  it("folds identical messages into one with a count", () => {
    const state = run(alert("Busy"), alert("Busy"), alert("Busy"));
    expect(state.dialogs).toHaveLength(1);
    expect(state.dialogs[0]).toMatchObject({ id: 1, count: 3 });
  });

  it("maps resume and progress flags by kind", () => {
    const { dialogs } = run(
      notification("a", { progress: "spinner" }),
      notification("b", { progress: "true" }),
      notification("c", { progress: true }),
      alert("d", { resume: "clear", progress: "spinner" }),
      alert("e", { resume: "resume" }),
    );
    expect(dialogs.map(({ progress, resume, clear }) => ({ progress, resume, clear }))).toEqual([
      { progress: "spinner", resume: false, clear: false },
      { progress: "bar", resume: false, clear: false },
      { progress: "bar", resume: false, clear: false },
      { progress: "none", resume: false, clear: true },
      { progress: "none", resume: false, clear: false },
    ]);
  });

  it("treats a modal without a type as a notification", () => {
    const { dialogs } = run({ type: "open", modal: { title: "Hello", message: "there" } });
    expect(dialogs[0]).toMatchObject({ kind: "notification", progress: "none" });
  });

  it("ignores server-rendered content modals", () => {
    const state = run({ type: "open", modal: { title: "Help", message: "<p>", modalType: "content" } });
    expect(state).toBe(initialDialogState);
  });

  it("closes by kind and title", () => {
    const opened = run(notification("spin", { progress: "spinner" }), alert("Oops"), notification("other"));
    const closed = dialogReducer(opened, { type: "close", kind: "notification", title: "Notification:" });
    expect(closed.dialogs.map((dialog) => dialog.message)).toEqual(["Oops"]);
    expect(dialogReducer(closed, { type: "close", kind: "alert", title: "Alert" }).dialogs).toEqual([]);
  });

  it("returns the same state when nothing matches", () => {
    const opened = run(alert("Oops"));
    expect(dialogReducer(opened, { type: "close", kind: "notification", title: "Alert" })).toBe(opened);
    expect(dialogReducer(opened, { type: "dismiss", id: 99 })).toBe(opened);
  });

  it("dismisses one dialog by id", () => {
    const opened = run(alert("one"), alert("two"));
    expect(dialogReducer(opened, { type: "dismiss", id: 1 }).dialogs.map((dialog) => dialog.message)).toEqual(["two"]);
  });
});
