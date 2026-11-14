import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderApp } from "../../test/render";

const sendAlert = (socket: ReturnType<typeof renderApp>["socket"], text: string) =>
  socket.serverMessage("activateModal", {
    title: "Alert",
    message: JSON.stringify(text),
    resume: "false",
    progress: "false",
    modalSize: "small",
    modalType: "alert",
  });

const sendNotification = (socket: ReturnType<typeof renderApp>["socket"], text: string, over: object = {}) =>
  socket.serverMessage("activateModal", {
    title: "Notification:",
    message: text,
    resume: "false",
    progress: "false",
    modalSize: "small",
    modalType: "notification",
    ...over,
  });

const backdrop = () => screen.queryByTestId("dialog-backdrop");

describe("ServerDialogs", () => {
  it("shows a server alert and closes it locally without telling the server", async () => {
    const { socket, user } = renderApp("/");
    socket.clearEmitted();
    sendAlert(socket, "Cannot issue command while sending gcode.");

    const dialog = screen.getByRole("alertdialog", { name: "Alert" });
    expect(dialog).toHaveAccessibleDescription("Cannot issue command while sending gcode.");
    expect(within(dialog).getByRole("button", { name: "Close" })).toHaveFocus();

    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    expect(backdrop()).not.toBeInTheDocument();
    expect(socket.emitted).toEqual([]);
  });

  it("survives rapid back-to-back alerts without leaving an overlay behind", async () => {
    const { socket, user } = renderApp("/");
    Array.from({ length: 12 }, (_, n) => n).forEach((n) => {
      sendAlert(socket, `Error ${n % 4}`);
      if (n % 3 === 0) sendNotification(socket, `Note ${n}`);
      if (n === 7) socket.serverMessage("closeModals", { title: "Notification:" });
    });

    expect(screen.getAllByRole("alertdialog")).toHaveLength(1);
    expect(document.querySelectorAll(".dialog-backdrop")).toHaveLength(1);
    expect(screen.getByText("Error 0")).toBeInTheDocument();
    expect(screen.getByText("(×3)")).toBeInTheDocument();

    const dismissAll = async (): Promise<void> => {
      const close = screen.queryByRole("button", { name: "Close" });
      if (!close) return;
      await user.click(close);
      return dismissAll();
    };
    await dismissAll();

    expect(backdrop()).not.toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    socket.serverMessage("statusMessage", { uploadFlag: 0, positioningMode: 0, currentTool: 0 });
    await user.click(screen.getByRole("button", { name: "Up" }));
    expect(socket.actions()).toContainEqual(["move", "up", 10]);
  });

  it("counts the messages still waiting", () => {
    const { socket } = renderApp("/");
    sendAlert(socket, "first");
    sendAlert(socket, "second");
    sendNotification(socket, "third");
    expect(screen.getByText("2 more waiting")).toBeInTheDocument();
  });

  it("offers Resume on tool-change notifications and sends resumeRun", async () => {
    const { socket, user } = renderApp("/");
    sendNotification(socket, "Change to tool 2", { resume: "resume" });
    const dialog = screen.getByRole("dialog", { name: "Notification" });
    const resume = within(dialog).getByRole("button", { name: "Resume" });
    expect(resume).toHaveFocus();

    await user.click(resume);
    expect(socket.actions()).toContainEqual(["resumeRun", "", ""]);
    expect(backdrop()).not.toBeInTheDocument();
  });

  it("offers Clear alert when asked and sends stopRun", async () => {
    const { socket, user } = renderApp("/");
    socket.serverMessage("activateModal", { title: "Alert", message: '"Jammed"', modalType: "alert", resume: "clear" });
    await user.click(screen.getByRole("button", { name: "Clear alert" }));
    expect(socket.actions()).toContainEqual(["stopRun", "", ""]);
    expect(backdrop()).not.toBeInTheDocument();
  });

  it("shows a spinner until the server closes the notification", () => {
    const { socket } = renderApp("/");
    sendNotification(socket, '"Stock Firmware Update in Progress, Please Wait."', { progress: "spinner" });
    expect(screen.getByRole("dialog")).toHaveAccessibleDescription("Stock Firmware Update in Progress, Please Wait.");
    expect(screen.getByRole("status", { name: "Working" })).toBeInTheDocument();

    socket.serverMessage("closeModals", { title: "Notification:" });
    expect(backdrop()).not.toBeInTheDocument();
  });

  it("shows a progress bar when asked", () => {
    const { socket } = renderApp("/");
    sendNotification(socket, "Working", { progress: "true" });
    expect(screen.getByRole("progressbar", { name: "Progress" })).toBeInTheDocument();
  });

  it("closes alerts on closeAlertModals but leaves notifications", () => {
    const { socket } = renderApp("/");
    sendAlert(socket, "Oops");
    sendNotification(socket, "Hello");
    socket.serverMessage("closeAlertModals", { title: "Alert" });
    expect(screen.getByRole("dialog", { name: "Notification" })).toBeInTheDocument();
  });

  it("closes with Escape and returns focus to where it was", async () => {
    const { socket, user } = renderApp("/");
    socket.serverMessage("statusMessage", { uploadFlag: 0, positioningMode: 0, currentTool: 0 });
    const play = screen.getByRole("button", { name: "Play" });
    play.focus();
    sendAlert(socket, "Oops");
    expect(screen.getByRole("button", { name: "Close" })).toHaveFocus();

    await user.keyboard("{Escape}");
    expect(backdrop()).not.toBeInTheDocument();
    expect(play).toHaveFocus();
  });

  it("keeps Tab inside the dialog and makes the page behind it inert", async () => {
    const { socket, user } = renderApp("/");
    socket.serverMessage("statusMessage", { uploadFlag: 0, positioningMode: 0, currentTool: 0 });
    sendNotification(socket, "Change to tool 2", { resume: "resume" });
    const dialog = screen.getByRole("dialog");
    const resume = within(dialog).getByRole("button", { name: "Resume" });
    const close = within(dialog).getByRole("button", { name: "Close" });
    expect(document.querySelector("main")?.closest("[inert]")).not.toBeNull();

    await user.tab();
    expect(close).toHaveFocus();
    await user.tab();
    expect(resume).toHaveFocus();
    await user.tab({ shift: true });
    expect(close).toHaveFocus();

    await user.click(close);
    expect(document.querySelector("[inert]")).toBeNull();
  });

  it("pulls focus back in and closes with Escape when focus has left the dialog", async () => {
    const { socket, user } = renderApp("/");
    sendAlert(socket, "Oops");
    const close = screen.getByRole("button", { name: "Close" });
    close.blur();

    await user.tab({ shift: true });
    expect(close).toHaveFocus();
    close.blur();
    await user.tab();
    expect(close).toHaveFocus();
    close.blur();
    await user.keyboard("{Escape}");
    expect(backdrop()).not.toBeInTheDocument();
  });

  it("ignores other keys", async () => {
    const { socket, user } = renderApp("/");
    sendAlert(socket, "Oops");
    await user.keyboard("a");
    expect(screen.getByRole("alertdialog")).toBeInTheDocument();
  });

  it("ignores content modals meant for this client and modals meant for others", () => {
    const { socket } = renderApp("/");
    socket.serverMessage("activateModal", { title: "Help", message: "<p>", modalType: "content", client: "client-1" });
    sendNotification(socket, "Not yours", { client: "someone-else" });
    expect(backdrop()).not.toBeInTheDocument();
  });

  it("stays available on every screen", () => {
    const { socket } = renderApp("/settings");
    sendAlert(socket, "Error updating setting");
    expect(screen.getByRole("alertdialog", { name: "Alert" })).toBeInTheDocument();
  });
});
