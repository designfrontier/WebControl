import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeFetch, startupState } from "../test/fakeFetch";
import { createFakeSocket } from "../test/fakeSocket";
import { renderApp } from "../test/render";

const status = () => within(screen.getByRole("list", { name: "Status" }));

describe("StatusBar", () => {
  it("shows connecting before the socket connects", () => {
    renderApp("/", { connect: false });
    expect(status().getByText("Server: connecting…")).not.toHaveAttribute("data-tone");
    expect(status().getByText("Controller: unknown")).not.toHaveAttribute("data-tone");
  });

  it("shows the host address from /api/state once connected", async () => {
    renderApp("/");
    await waitFor(() => expect(status().getByText("Server: connected (192.168.1.20:5000)")).toBeInTheDocument());
  });

  it("omits the placeholder host address the server uses before it knows its own", () => {
    const socket = createFakeSocket();
    renderApp("/", { socket, machine: { hostAddress: "-" }, fetch: () => new Promise(() => {}) });
    expect(status().getByText("Server: connected")).toBeInTheDocument();
  });

  it("shows not connected after the socket drops", () => {
    const { socket } = renderApp("/");
    socket.serverDisconnect();
    expect(status().getByText("Server: not connected")).toHaveAttribute("data-tone", "bad");
  });

  it("follows the controller connection", () => {
    const { socket } = renderApp("/");
    socket.serverMessage("connectionStatus", { status: "disconnected", port: "none", fakeServoStatus: false });
    expect(status().getByText("Controller: not connected")).toHaveAttribute("data-tone", "bad");

    socket.serverMessage("connectionStatus", { status: "connected", port: "COM3", fakeServoStatus: false });
    expect(status().getByText("Controller: COM3")).toHaveAttribute("data-tone", "ok");

    socket.serverMessage("connectionStatus", { status: "connected", port: "COM3", fakeServoStatus: true });
    expect(status().getByText("Controller: COM3 / fake servo on")).toBeInTheDocument();
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
