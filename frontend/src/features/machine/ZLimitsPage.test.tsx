import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeFetch } from "../../test/fakeFetch";
import { renderApp } from "../../test/render";
import { machineSetup } from "./fixtures";

const LIMIT_BUTTONS = ["Clear Z limits", "Save Z max", "Save Z min", "Report Z limits"];

const renderLimits = (over = {}) =>
  renderApp("/machine/z-limits", { fetch: fakeFetch({ "/api/machine/setup": machineSetup(over) }) });

describe("ZLimitsPage", () => {
  it("clears, saves and reports the limits on firmware that supports them", async () => {
    const { socket, user } = renderLimits();
    await waitFor(() => expect(screen.getByRole("button", { name: "Clear Z limits" })).toBeEnabled());

    await user.click(screen.getByRole("button", { name: "Clear Z limits" }));
    await user.click(screen.getByRole("button", { name: "↑ Raise" }));
    await user.click(screen.getByRole("button", { name: "Save Z max" }));
    await user.click(screen.getByRole("button", { name: "Save Z min" }));
    await user.click(screen.getByRole("button", { name: "Report Z limits" }));
    expect(socket.actions()).toEqual([
      ["clearZ", "", ""],
      ["moveZ", "raise", 1],
      ["setMaxZ", "", ""],
      ["setMinZ", "", ""],
      ["getZlimits", "", ""],
    ]);
  });

  it("explains and locks the limit buttons on firmware before 51.292", async () => {
    renderLimits({ firmwareSupportsZaxisLimit: false, controllerFirmwareVersion: 51.29, firmwareBuild: "51.29x" });
    expect(await screen.findByText(/need holey firmware 51.292 or later/)).toHaveTextContent("reports 51.29x.");
    LIMIT_BUTTONS.forEach((name) => expect(screen.getByRole("button", { name })).toBeDisabled());
    expect(screen.getByRole("button", { name: "↑ Raise" })).toBeEnabled();
  });

  it("says unknown when no controller has reported a version", async () => {
    renderLimits({ firmwareSupportsZaxisLimit: false, controllerFirmwareVersion: 0, firmwareBuild: null });
    expect(await screen.findByText(/need holey firmware/)).toHaveTextContent("reports no version yet.");
  });

  it("warns that zeroing Z moves the saved limits", async () => {
    renderLimits();
    expect(await screen.findByText(/Zeroing Z shifts the saved limits/)).toBeInTheDocument();
  });

  it("shows the latest controller messages", async () => {
    const { socket } = renderLimits();
    expect(screen.getByText("Nothing from the controller yet.")).toBeInTheDocument();
    ["a", "b", "c", "d", "e", "f"].forEach((line) => socket.serverMessage("controllerMessage", `line ${line}`));
    const log = screen.getByRole("log");
    expect(
      within(log)
        .getAllByRole("listitem")
        .map((item) => item.textContent),
    ).toEqual(["line b", "line c", "line d", "line e", "line f"]);
    await screen.findByRole("button", { name: "Clear Z limits" });
  });
});
