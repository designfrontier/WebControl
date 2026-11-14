import { screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeFetch, jsonResponse } from "../../test/fakeFetch";
import { renderApp } from "../../test/render";
import { machineSetup } from "./fixtures";

const setupFetch = (over = {}) => fakeFetch({ "/api/machine/setup": machineSetup(over) });

const renderZAxis = (options: Parameters<typeof renderApp>[1] = {}) =>
  renderApp("/machine/z-axis", { fetch: setupFetch(), ...options });

const step = () => screen.getByRole("textbox", { name: "Step" });

describe("ZAxisPage", () => {
  it("raises and lowers by the step from /api/state", async () => {
    const { socket, user } = renderZAxis();
    expect(screen.getByRole("heading", { level: 2, name: "Z-axis" })).toBeInTheDocument();
    await waitFor(() => expect(step()).toHaveValue("1"));

    await user.click(screen.getByRole("button", { name: "↑ Raise" }));
    await user.click(screen.getByRole("button", { name: "↓ Lower" }));
    await user.click(screen.getByRole("button", { name: "Stop Z" }));
    expect(socket.actions()).toEqual([
      ["moveZ", "raise", 1],
      ["moveZ", "lower", 1],
      ["stopZ", "", ""],
    ]);
  });

  it("moves by a typed step and blocks invalid ones", async () => {
    const { socket, user } = renderZAxis();
    await waitFor(() => expect(step()).toHaveValue("1"));

    await user.clear(step());
    expect(screen.getByText("Enter a step greater than zero.")).toBeInTheDocument();
    expect(step()).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByRole("button", { name: "↑ Raise" })).toBeDisabled();

    await user.type(step(), "2.5");
    await user.click(screen.getByRole("button", { name: "↓ Lower" }));
    expect(socket.actions()).toContainEqual(["moveZ", "lower", 2.5]);
  });

  it("switches units with the converted step and follows the server's answer", async () => {
    const { socket, user } = renderZAxis();
    await waitFor(() => expect(step()).toHaveValue("1"));
    await user.clear(step());
    await user.type(step(), "25.4");

    await user.click(screen.getByRole("button", { name: "Units: mm, switch" }));
    expect(socket.emittedEvents("updateSetting")).toEqual([{ data: { setting: "toInchesZ", value: "1.00" } }]);

    socket.serverMessage("requestedSetting", { setting: "unitsZ", value: "INCHES" });
    socket.serverMessage("requestedSetting", { setting: "distToMoveZ", value: "1.00" });
    expect(screen.getByRole("button", { name: "Units: in, switch" })).toHaveTextContent("in");
    expect(step()).toHaveValue("1");

    await user.click(screen.getByRole("button", { name: "Units: in, switch" }));
    expect(socket.emittedEvents("updateSetting")[1]).toEqual({ data: { setting: "toMMZ", value: "25.40" } });
  });

  it("falls back to the stored step when switching units from an invalid entry", async () => {
    const { socket, user } = renderZAxis();
    await waitFor(() => expect(step()).toHaveValue("1"));
    await user.clear(step());
    await user.click(screen.getByRole("button", { name: "Units: mm, switch" }));
    expect(socket.emittedEvents("updateSetting")).toEqual([{ data: { setting: "toInchesZ", value: "0.04" } }]);
  });

  it("waits for units before allowing a switch", () => {
    renderZAxis({ fetch: () => new Promise(() => {}) });
    expect(screen.getByRole("button", { name: "Units loading" })).toBeDisabled();
    expect(step()).toHaveValue("");
  });

  it("defines Z zero, and touches off only with a touch plate", async () => {
    const { socket, user } = renderZAxis();
    await screen.findByText("Touch off needs a touch plate.");
    expect(screen.getByRole("button", { name: "Touch off Z" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Define Z zero" }));
    expect(socket.actions()).toEqual([["defineZ0", "", ""]]);
  });

  it("touches off when the machine has a touch plate", async () => {
    const { socket, user } = renderZAxis({ fetch: setupFetch({ touchPlate: true }) });
    const touch = screen.getByRole("button", { name: "Touch off Z" });
    await waitFor(() => expect(touch).toBeEnabled());
    await user.click(touch);
    expect(socket.actions()).toEqual([["touchZ", "", ""]]);
  });

  it("locks moves but not Stop while G-code runs", async () => {
    renderZAxis({ machine: { status: { uploadFlag: 1, positioningMode: 0, currentTool: 0 } } });
    await waitFor(() => expect(step()).toHaveValue("1"));
    expect(screen.getByText("Z moves are disabled while G-code is running.")).toBeInTheDocument();
    ["↑ Raise", "↓ Lower", "Define Z zero"].forEach((name) =>
      expect(screen.getByRole("button", { name })).toBeDisabled(),
    );
    expect(screen.getByRole("button", { name: "Stop Z" })).toBeEnabled();
  });

  it("reports a failed setup load and retries it", async () => {
    let fail = true;
    const fetch = fakeFetch({
      "/api/machine/setup": () => (fail ? jsonResponse({}, 500) : jsonResponse(machineSetup())),
    });
    const { user } = renderZAxis({ fetch });
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not load machine setup: GET /api/machine/setup failed with 500",
    );
    fail = false;
    await user.click(screen.getByRole("button", { name: "Retry" }));
    await screen.findByText("Touch off needs a touch plate.");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});
