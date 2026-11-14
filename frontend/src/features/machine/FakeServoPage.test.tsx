import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderApp } from "../../test/render";

describe("FakeServoPage", () => {
  it("shows the controller's fake servo state", () => {
    const { socket } = renderApp("/machine/fake-servo");
    expect(screen.getByText("The controller is not connected.")).toBeInTheDocument();
    socket.serverMessage("connectionStatus", { status: "connected", port: "COM3", fakeServoStatus: true });
    expect(screen.getByText("Fake servo is on.")).toBeInTheDocument();
    socket.serverMessage("connectionStatus", { status: "connected", port: "COM3", fakeServoStatus: false });
    expect(screen.getByText("Fake servo is off.")).toBeInTheDocument();
  });

  it("disables fake servo", async () => {
    const { socket, user } = renderApp("/machine/fake-servo");
    await user.click(screen.getByRole("button", { name: "Disable fake servo" }));
    expect(socket.actions()).toEqual([["setFakeServo", 0, ""]]);
    expect(screen.getByText("Asked the controller to disable fake servo.")).toBeInTheDocument();
  });

  it("enables fake servo only with the secret code", async () => {
    const { socket, user } = renderApp("/machine/fake-servo");
    const code = screen.getByLabelText("Secret code");

    await user.type(code, "41");
    await user.click(screen.getByRole("button", { name: "Enable fake servo" }));
    expect(socket.actions()).toEqual([]);
    expect(screen.getByText(/That code is not right/)).toBeInTheDocument();

    await user.clear(code);
    await user.type(code, "42{Enter}");
    expect(socket.actions()).toEqual([["setFakeServo", 1, ""]]);
    expect(code).toHaveValue("");
    expect(screen.getByText("Asked the controller to enable fake servo.")).toBeInTheDocument();
  });
});
