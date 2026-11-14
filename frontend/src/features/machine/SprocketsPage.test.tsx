import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { fakeFetch, jsonResponse } from "../../test/fakeFetch";
import { currentPath, renderApp } from "../../test/render";
import { machineSetup } from "./fixtures";

const setupFetch = (over = {}) => fakeFetch({ "/api/machine/setup": machineSetup(over) });
const button = (name: string | RegExp) => screen.getByRole("button", { name });

describe("SprocketsPage", () => {
  it("turns each sprocket by the old template's signed degrees", async () => {
    const { socket, user } = renderApp("/machine/sprockets", { fetch: setupFetch() });
    await screen.findByRole("button", { name: "Extend left chain to 1650 mm" });
    expect(screen.queryByRole("group", { name: /Bottom left motor/ })).not.toBeInTheDocument();

    await user.click(button("Left motor clockwise 360°"));
    await user.click(button("Left motor counter-clockwise 0.1°"));
    await user.click(button("Right motor clockwise 10°"));
    await user.click(button("Right motor counter-clockwise 1°"));
    expect(socket.actions()).toEqual([
      ["setSprockets", "L", 360],
      ["setSprockets", "L", -0.1],
      ["setSprockets", "R", -10],
      ["setSprockets", "R", 1],
    ]);
    expect(button("Left motor clockwise 360°")).toHaveTextContent("360°");
  });

  it("shows the bottom motors on four-motor firmware", async () => {
    const { socket, user } = renderApp("/machine/sprockets", { fetch: setupFetch({ fourMotor: true }) });
    await user.click(await screen.findByRole("button", { name: "Bottom left motor clockwise 1°" }));
    await user.click(button("Bottom right motor clockwise 1°"));
    expect(socket.actions()).toEqual([
      ["setSprockets", "M", 1],
      ["setSprockets", "S", -1],
    ]);
  });

  it("unlocks extending only after Define zero, and any turn locks it again", async () => {
    const { socket, user } = renderApp("/machine/sprockets", { fetch: setupFetch() });
    const left = await screen.findByRole("button", { name: "Extend left chain to 1650 mm" });
    expect(left).toBeDisabled();

    await user.click(button("Define zero"));
    expect(left).toBeEnabled();
    expect(button("Extend right chain to 1650 mm")).toBeEnabled();

    await user.click(button("Right motor clockwise 1°"));
    expect(left).toBeDisabled();

    expect(socket.actions().map(([command]) => command)).toEqual(["setSprocketsZero", "setSprockets"]);
  });

  it("does not offer the automatic alignment upstream retired", async () => {
    renderApp("/machine/sprockets", { fetch: setupFetch() });
    await screen.findByRole("button", { name: "Extend left chain to 1650 mm" });
    expect(screen.queryByRole("button", { name: /automatic/i })).not.toBeInTheDocument();
  });

  it("counts down to the extend like the server does", async () => {
    const { socket } = renderApp("/machine/sprockets", { fetch: setupFetch() });
    await screen.findByRole("button", { name: "Extend left chain to 1650 mm" });
    vi.useFakeTimers();
    fireEvent.click(button("Define zero"));
    fireEvent.click(button("Extend left chain to 1650 mm"));
    expect(socket.actions()).toContainEqual(["adjustChain", "left", ""]);

    const status = screen.getByText(/chain starts extending/);
    expect(status).toHaveTextContent("The left chain starts extending in 6 s.");
    act(() => vi.advanceTimersByTime(5000));
    expect(status).toHaveTextContent("in 1 s.");
    act(() => vi.advanceTimersByTime(1000));
    expect(status).toHaveTextContent("The left chain started extending.");
    act(() => vi.advanceTimersByTime(3000));
    expect(status).toHaveTextContent("The left chain started extending.");
    fireEvent.click(button("Stop"));
    expect(status).toBeEmptyDOMElement();

    fireEvent.click(button("Extend right chain to 1650 mm"));
    expect(status).toHaveTextContent("The right chain starts extending in 6 s.");
    fireEvent.click(button("Left motor clockwise 1°"));
    expect(status).toBeEmptyDOMElement();
  });

  it("stops and runs motors by time", async () => {
    const { socket, user } = renderApp("/machine/sprockets", { fetch: setupFetch() });
    await user.click(button("Stop"));
    await user.click(screen.getByText("Run motors without encoders"));
    await user.click(button("Run left motor clockwise 10 s"));
    await user.click(button("Run right motor counterclockwise 0.1 s"));
    expect(socket.actions()).toEqual([
      ["stopRun", "", ""],
      ["rotateSprocket", "L", 10],
      ["rotateSprocket", "R", 0.1],
    ]);
  });

  it("refetches the setup when the controller connects", async () => {
    const fetch = setupFetch();
    const { socket } = renderApp("/machine/sprockets", { fetch });
    await screen.findByRole("button", { name: "Extend left chain to 1650 mm" });
    const setupCalls = () => fetch.mock.calls.filter(([url]) => url === "/api/machine/setup").length;
    const before = setupCalls();
    socket.serverMessage("connectionStatus", { status: "connected", port: "COM3" });
    await waitFor(() => expect(setupCalls()).toBe(before + 1));
  });

  it("refetches the setup once the controller reports its firmware", async () => {
    let reported = machineSetup();
    const fetch = fakeFetch({ "/api/machine/setup": () => jsonResponse(reported) });
    const setupCalls = () => fetch.mock.calls.filter(([url]) => url === "/api/machine/setup").length;
    const { socket } = renderApp("/machine/sprockets", { fetch });
    await screen.findByRole("button", { name: "Extend left chain to 1650 mm" });
    socket.serverMessage("connectionStatus", { status: "connected", port: "COM3" });
    await waitFor(() => expect(setupCalls()).toBeGreaterThan(1));
    expect(screen.queryByRole("button", { name: "Bottom left motor clockwise 1°" })).not.toBeInTheDocument();

    reported = machineSetup({ fourMotor: true });
    socket.serverMessage("clearAlarm", { data: "" });
    expect(await screen.findByRole("button", { name: "Bottom left motor clockwise 1°" })).toBeInTheDocument();
  });
});

describe("ResetChainsPage", () => {
  it("jogs sprockets, stops, and confirms the reset before returning to the list", async () => {
    const { socket, user } = renderApp("/machine/reset-chains", { fetch: setupFetch() });
    expect(screen.getByText(/both chains are extended to the extend chain distance/)).toBeInTheDocument();
    await screen.findByText(/both chains are extended to 1650 mm/);

    await user.click(button("Left motor clockwise 10°"));
    await user.click(button("Stop"));
    await user.click(button(/I have reset both chains/));
    expect(socket.actions()).toEqual([
      ["setSprockets", "L", 10],
      ["stopRun", "", ""],
      ["setSprocketsDefault", "", ""],
    ]);
    expect(currentPath()).toBe("/machine");
  });
});
