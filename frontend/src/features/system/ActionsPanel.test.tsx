import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { jsonResponse } from "../../test/fakeFetch";
import { renderApp } from "../../test/render";
import { systemFetch, systemInfo } from "./systemFixtures";

const group = (name: string) => within(screen.getByRole("region", { name }));
const dialog = () => screen.getByRole("dialog");
const notice = () => group("Actions").getByRole("status");

const renderActions = (options: Parameters<typeof renderApp>[1] = {}) =>
  renderApp("/system", { fetch: systemFetch(), ...options });

describe("ActionsPanel", () => {
  it("lists the classic action menu, firmware versions included once info loads", async () => {
    renderActions();
    expect(screen.getByRole("heading", { name: "Actions", level: 2 })).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: "Upgrade stock firmware (1.26)" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Upgrade holey firmware (51.28)" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Upgrade beta firmware (100.07)" })).toBeInTheDocument();
    expect(group("Diagnostics and maintenance").getByRole("button", { name: "Shut down WebControl" })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Turn off Raspberry Pi" })).not.toBeInTheDocument();
  });

  it("labels firmware buttons without versions until info arrives", () => {
    renderActions({ fetch: systemFetch({ "/api/system/info": () => new Promise<Response>(() => {}) }) });
    expect(screen.getByRole("button", { name: "Upgrade stock firmware" })).toBeInTheDocument();
  });

  it("offers turning off the Pi on Pi and PyInstaller builds", async () => {
    renderActions({ fetch: systemFetch({ "/api/system/info": systemInfo({ canTurnOff: true }) }) });
    expect(await screen.findByRole("button", { name: "Turn off Raspberry Pi" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Shut down WebControl/WebMCP" })).toBeInTheDocument();
  });

  it("sends harmless actions straight away and says so", async () => {
    const { user, socket } = renderActions();
    await user.click(screen.getByRole("button", { name: "Sync settings" }));
    expect(socket.actions()).toEqual([["reportSettings", "", ""]]);
    expect(notice()).toHaveTextContent("Sent “Sync settings”.");
  });

  it("asks before destructive actions and sends the wipe extent on confirm", async () => {
    const { user, socket } = renderActions();
    await user.click(screen.getByRole("button", { name: "Wipe controller EEPROM" }));

    expect(dialog()).toHaveAccessibleName("Wipe controller EEPROM?");
    expect(within(dialog()).getByRole("button", { name: "Cancel" })).toHaveFocus();
    expect(socket.actions()).toEqual([]);

    await user.click(within(dialog()).getByRole("button", { name: "Wipe controller EEPROM" }));
    expect(socket.actions()).toEqual([["wipeEEPROM", "All", ""]]);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it.each([
    ["Test motors and encoders", "Run motor test", "testMotors"],
    ["Clear log files", "Clear logs", "clearLogs"],
    ["Reset home to center", "Reset home", "resetHomeToCenter"],
    ["Shut down WebControl", "Shut down", "shutdown"],
  ])("confirms %s", async (label, confirm, command) => {
    const { user, socket } = renderActions();
    await user.click(screen.getByRole("button", { name: label }));
    await user.click(within(dialog()).getByRole("button", { name: confirm }));
    expect(socket.actions()).toEqual([[command, "", ""]]);
  });

  it("cancelling sends nothing and leaves the page usable, unlike the classic stuck backdrop", async () => {
    const { user, socket } = renderActions();
    const trigger = screen.getByRole("button", { name: "Upgrade stock firmware" });
    await user.click(trigger);
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();

    await user.click(screen.getByRole("button", { name: "Clear log files" }));
    await user.click(document.querySelector(".dialog-backdrop")!);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Return to center" }));
    expect(socket.actions()).toEqual([["returnToCenter", "", ""]]);
  });

  it("disables actions the server would refuse while a job runs", () => {
    renderActions({ machine: { status: { uploadFlag: 1, positioningMode: 0, currentTool: 0 } } });
    expect(screen.getByText(/A job is running/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Test motors and encoders" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Clear log files" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Query camera" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Shut down WebControl" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Restore WebControl…" })).toBeDisabled();
  });

  it("allows clearing logs while paused", () => {
    renderActions({ machine: { status: { uploadFlag: 2, positioningMode: 0, currentTool: 0 } } });
    expect(screen.getByRole("button", { name: "Clear log files" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Sync settings" })).toBeDisabled();
  });

  it("links diagnostics and backup downloads with a fresh cache buster on click", async () => {
    const { user } = renderActions();
    const link = screen.getByRole("link", { name: "Download diagnostics file" });
    expect(link).toHaveAttribute("href", "/downloadDiagnostics?rnd=0");
    link.addEventListener("click", (event) => event.preventDefault());
    await user.click(link);
    expect(link.getAttribute("href")).toMatch(/^\/downloadDiagnostics\?rnd=[1-9]\d+$/);
    expect(screen.getByRole("link", { name: "Back up WebControl" })).toHaveAttribute("target", "_blank");
  });

  it("links to PID tuning and the sections that own calibration", async () => {
    const { user } = renderActions();
    expect(group("Positioning").getByRole("link", { name: "Calibration" })).toHaveAttribute("href", "/calibration");
    await user.click(screen.getByRole("link", { name: "PID tuning (advanced)" }));
    expect(screen.getByRole("heading", { name: "PID tuning", level: 2 })).toBeInTheDocument();
  });
});

describe("Restore dialog", () => {
  const zip = new File(["zip"], "wc_backup.zip", { type: "application/zip" });

  it("uploads the chosen backup and reports success", async () => {
    const fetch = systemFetch({ "POST /importRestoreWebControl": { status: 200 } });
    const { user } = renderActions({ fetch });
    await user.click(screen.getByRole("button", { name: "Restore WebControl…" }));

    const restore = within(dialog()).getByRole("button", { name: "Restore backup" });
    expect(restore).toBeDisabled();
    await user.upload(within(dialog()).getByLabelText("Backup file"), zip);
    await user.click(restore);

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(notice()).toHaveTextContent("Backup restored.");
    expect(fetch).toHaveBeenCalledWith("/importRestoreWebControl", expect.objectContaining({ method: "POST" }));
  });

  it("shows the failure and lets the user retry or cancel", async () => {
    const fetch = systemFetch({ "POST /importRestoreWebControl": () => jsonResponse({ status: 500 }, 500) });
    const { user } = renderActions({ fetch });
    await user.click(screen.getByRole("button", { name: "Restore WebControl…" }));
    await user.upload(within(dialog()).getByLabelText("Backup file"), zip);
    await user.click(within(dialog()).getByRole("button", { name: "Restore backup" }));

    expect(await within(dialog()).findByRole("alert")).toHaveTextContent("Restore failed with 500");
    expect(within(dialog()).getByRole("button", { name: "Restore backup" })).toBeEnabled();
    await user.click(within(dialog()).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("does nothing without a file", async () => {
    const fetch = systemFetch();
    const { user } = renderActions({ fetch });
    await user.click(screen.getByRole("button", { name: "Restore WebControl…" }));
    await user.upload(within(dialog()).getByLabelText("Backup file"), []);
    expect(within(dialog()).getByRole("button", { name: "Restore backup" })).toBeDisabled();
  });
});

describe("Fake servo dialog", () => {
  it("disables fake servo at once and shows the current state", async () => {
    const { user, socket } = renderActions({ connect: false });
    socket.serverConnect();
    socket.serverMessage("connectionStatus", { status: "connected", port: "COM3", fakeServoStatus: true });
    await user.click(screen.getByRole("button", { name: "Fake servo (advanced)…" }));
    expect(within(dialog()).getByText("Fake servo is on.")).toBeInTheDocument();

    await user.click(within(dialog()).getByRole("button", { name: "Disable fake servo" }));
    expect(socket.actions()).toEqual([["setFakeServo", 0, ""]]);
    expect(notice()).toHaveTextContent("disable fake servo");
  });

  it("enables fake servo only with the secret code", async () => {
    const { user, socket } = renderActions();
    await user.click(screen.getByRole("button", { name: "Fake servo (advanced)…" }));
    expect(within(dialog()).getByText("Fake servo is off.")).toBeInTheDocument();
    const enable = within(dialog()).getByRole("button", { name: "Enable fake servo" });

    await user.type(within(dialog()).getByLabelText("Secret code to enable"), "41");
    expect(enable).toBeDisabled();
    await user.clear(within(dialog()).getByLabelText("Secret code to enable"));
    await user.type(within(dialog()).getByLabelText("Secret code to enable"), "42");
    await user.click(enable);

    expect(socket.actions()).toEqual([["setFakeServo", 1, ""]]);
    expect(notice()).toHaveTextContent("enable fake servo");
  });

  it("closes on cancel", async () => {
    const { user } = renderActions();
    await user.click(screen.getByRole("button", { name: "Fake servo (advanced)…" }));
    await user.click(within(dialog()).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});
