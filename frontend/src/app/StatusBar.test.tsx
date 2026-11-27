import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeFetch, startupState } from "../test/fakeFetch";
import { createFakeSocket } from "../test/fakeSocket";
import { renderApp } from "../test/render";

const status = () => within(screen.getByRole("list", { name: "Status" }));
/** The chip whose full text this is. */
const chip = (text: string) => status().getByText(text).closest("li");

describe("StatusBar", () => {
  it("shows connecting before the socket connects", () => {
    renderApp("/", { connect: false });
    expect(chip("Server: connecting…")).not.toHaveAttribute("data-tone");
    expect(chip("Controller: unknown")).not.toHaveAttribute("data-tone");
  });

  it("shows the host address from /api/state once connected", async () => {
    renderApp("/");
    await waitFor(() => expect(chip("Server: connected")).toHaveAttribute("title", "WebControl at 192.168.1.20:5000"));
  });

  it("omits the placeholder host address the server uses before it knows its own", () => {
    const socket = createFakeSocket();
    renderApp("/", { socket, machine: { hostAddress: "-" }, fetch: () => new Promise(() => {}) });
    expect(chip("Server: connected")).not.toHaveAttribute("title");
  });

  it("shows not connected after the socket drops", () => {
    const { socket } = renderApp("/");
    socket.serverDisconnect();
    expect(chip("Server: not connected")).toHaveAttribute("data-tone", "bad");
  });

  it("follows the controller connection", () => {
    const { socket } = renderApp("/");
    socket.serverMessage("connectionStatus", { status: "disconnected", port: "none", fakeServoStatus: false });
    expect(chip("Controller: not connected")).toHaveAttribute("data-tone", "bad");

    socket.serverMessage("connectionStatus", { status: "connected", port: "COM3", fakeServoStatus: false });
    expect(chip("Controller: COM3")).toHaveAttribute("data-tone", "ok");

    socket.serverMessage("connectionStatus", { status: "connected", port: "COM3", fakeServoStatus: true });
    expect(chip("Controller: COM3 / fake servo on")).toBeInTheDocument();
  });

  it("gives narrow headers short forms that keep the build commit, hidden from screen readers", async () => {
    const { socket } = renderApp("/", {
      fetch: fakeFetch({
        "GET /api/state": startupState({ version: "0.2.0", commit: "7dd5305", controllerFirmwareVersion: 51.27 }),
      }),
    });
    socket.serverMessage("connectionStatus", { status: "connected", port: "COM3", fakeServoStatus: true });
    socket.serverMessage("healthMessage", { cpuUsage: 7.6, bufferSize: -1, controllerFirmwareVersion: 51.27 });
    await status().findByText("WebControl 0.2.0 · 7dd5305");
    const list = screen.getByRole("list", { name: "Status" });
    const texts = (selector: string) => Array.from(list.querySelectorAll(selector)).map((form) => form.textContent);
    expect(texts(".chip-short")).toEqual(["Server", "COM3 / fake servo on", "v0.2.0·7dd5305"]);
    expect(texts(".chip-shortest")).toEqual(["51.27", "7dd5305", "8%"]);
    list
      .querySelectorAll(".chip-short, .chip-shortest")
      .forEach((form) => expect(form).toHaveAttribute("aria-hidden", "true"));

    socket.serverMessage("connectionStatus", { status: "disconnected", port: "none", fakeServoStatus: false });
    expect(chip("Controller: not connected")?.querySelector(".chip-short")).toHaveTextContent("Controller");
  });

  it("shows CPU, and buffer only when the controller is buffering", () => {
    const { socket } = renderApp("/");
    socket.serverMessage("healthMessage", { cpuUsage: 41.6, bufferSize: -1 });
    expect(status().getByText("CPU 42%")).toBeInTheDocument();
    expect(status().queryByText(/^Buffer/)).not.toBeInTheDocument();

    socket.serverMessage("healthMessage", { cpuUsage: 10, bufferSize: 96 });
    expect(status().getByText("Buffer 96")).toBeInTheDocument();
  });
});

describe("firmware chip", () => {
  it.each([
    [0, "Firmware: unknown"],
    [1.26, "Stock 1.26"],
    [51.27, "Holey 51.27"],
    [100.07, "Custom 100.07"],
  ])("reads %s as %s", async (version, text) => {
    const { socket } = renderApp("/");
    socket.serverMessage("healthMessage", { cpuUsage: 1, bufferSize: -1, controllerFirmwareVersion: version });
    expect(status().getByText(text)).toBeInTheDocument();
  });

  it("starts from the version in /api/state", async () => {
    renderApp("/", { fetch: fakeFetch({ "GET /api/state": startupState({ controllerFirmwareVersion: 51.27 }) }) });
    expect(await status().findByText("Holey 51.27")).toBeInTheDocument();
  });

  it("shows the build text, so 51.30 does not read as 51.3", async () => {
    const state = startupState({ controllerFirmwareVersion: 51.29, firmwareBuild: "51.292" });
    const { socket } = renderApp("/", { fetch: fakeFetch({ "GET /api/state": state }) });
    expect(await status().findByText("Holey 51.292")).toBeInTheDocument();
    const health = { cpuUsage: 1, bufferSize: -1, controllerFirmwareVersion: 51.3, firmwareBuild: "51.30" };
    socket.serverMessage("healthMessage", health);
    expect(status().getByText("Holey 51.30")).toBeInTheDocument();
    socket.serverMessage("healthMessage", { cpuUsage: 1, bufferSize: -1 });
    expect(status().getByText("Holey 51.30")).toBeInTheDocument();
  });
});

describe("software chip", () => {
  it("shows the WebControl version and build commit beside the firmware", async () => {
    renderApp("/", { fetch: fakeFetch({ "GET /api/state": startupState({ version: "0.2.0", commit: "7dd5305" }) }) });
    const software = (await status().findByText("WebControl 0.2.0 · 7dd5305")).closest("li");
    expect(software).toHaveAttribute("title", "WebControl 0.2.0 · 7dd5305: the WebControl version and build");
    expect(software?.previousElementSibling).toHaveTextContent("Firmware: unknown");
  });

  it("shows the version alone when the backend sends no commit", async () => {
    renderApp("/", { fetch: fakeFetch({ "GET /api/state": startupState({ version: "0.2.0" }) }) });
    const software = (await status().findByText("WebControl 0.2.0")).closest("li");
    expect(software?.querySelector(".chip-short")).toHaveTextContent(/^v0\.2\.0$/);
    expect(software?.querySelector(".chip-shortest")).toHaveTextContent(/^v0\.2\.0$/);
  });

  it("is absent until a backend reports its version", async () => {
    renderApp("/", { fetch: fakeFetch({ "GET /api/state": startupState({ controllerFirmwareVersion: 51.27 }) }) });
    await status().findByText("Holey 51.27");
    expect(status().queryByText(/^WebControl /)).not.toBeInTheDocument();
  });
});

describe("AlarmBanner", () => {
  it("announces alarms on the calibration page and hides them when cleared", () => {
    const { socket } = renderApp("/calibration");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();

    socket.serverMessage("alarm", { message: "Alarm: Sled Not Keeping Up" });
    expect(screen.getByRole("alert")).toHaveTextContent("Alarm: Sled Not Keeping Up");

    socket.serverMessage("clearAlarm", { data: "" });
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});
