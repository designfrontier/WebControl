import { screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { jsonResponse } from "../../test/fakeFetch";
import { currentPath, renderApp } from "../../test/render";
import { systemFetch, systemInfo } from "../../test/systemFixtures";

vi.mock("socket.io-client", () => ({
  io: () => ({ id: undefined, connected: false, on: vi.fn(), disconnect: vi.fn() }),
}));

const helpMenu = () => screen.getByRole("button", { name: "Help" });

describe("system modals", () => {
  it.each([
    ["/system", "/actions"],
    ["/system/pid", "/pid"],
    ["/system/about", "/about"],
    ["/system/updates", "/updates"],
    ["/system/help/GettingStarted/firstSteps.md", "/help/GettingStarted/firstSteps.md"],
    ["/files", "/"],
  ])("keeps the old address %s working as %s", (from, to) => {
    renderApp(from, { fetch: systemFetch() });
    expect(currentPath()).toBe(to);
  });

  it("serves the logs as a page of their own, without the app chrome", () => {
    renderApp("/system/logs", { fetch: systemFetch() });
    expect(currentPath()).toBe("/logs");
    expect(screen.getByRole("heading", { name: "WebControl logs", level: 1 })).toBeInTheDocument();
    expect(screen.queryByRole("navigation", { name: "Primary" })).not.toBeInTheDocument();
  });

  it("shows version and firmware on About", async () => {
    renderApp("/about", { fetch: systemFetch() });
    const about = within(screen.getByRole("dialog", { name: "About" }));
    expect(await about.findByText("v0.95")).toBeInTheDocument();
    expect(about.getByText("RPI")).toBeInTheDocument();
    expect(about.getByText("1.26")).toBeInTheDocument();
    expect(about.getByRole("link", { name: /github\.com\/WebControlCNC/ })).toHaveAttribute("target", "_blank");
  });

  it("shows placeholders on About until info loads, and unreported firmware", async () => {
    const pending = renderApp("/about", {
      fetch: systemFetch({ "/api/system/info": () => new Promise(() => {}) }),
    });
    expect(within(screen.getByRole("dialog", { name: "About" })).getAllByText("…")).toHaveLength(3);
    pending.unmount();

    renderApp("/about", {
      fetch: systemFetch({ "/api/system/info": systemInfo({ controllerFirmwareVersion: 0 }) }),
    });
    expect(await screen.findByText("Not reported")).toBeInTheDocument();
  });

  it("badges Help and Update when the server reports an update", async () => {
    const { user } = renderApp("/", {
      fetch: systemFetch({ "/api/system/info": systemInfo({ update: { available: true, version: "v0.96" } }) }),
    });
    expect(await within(helpMenu()).findByText("New")).toBeInTheDocument();
    await user.click(helpMenu());
    expect(within(screen.getByRole("menuitem", { name: "Update" })).getByText("New")).toBeInTheDocument();
  });

  it("badges Help when pyinstallUpdate arrives, and the Update modal says so", async () => {
    const { socket, user } = renderApp("/", { fetch: systemFetch() });
    expect(within(helpMenu()).queryByText("New")).not.toBeInTheDocument();
    await user.click(helpMenu());
    await user.click(screen.getByRole("menuitem", { name: "Update" }));
    socket.serverMessage("pyinstallUpdate", "on");
    expect(within(screen.getByRole("button", { name: "Help", hidden: true })).getByText("New")).toBeInTheDocument();
    expect(within(screen.getByRole("dialog", { name: "Update" })).getByText("An update is available")).toBeVisible();
  });

  it("retries when system info fails", async () => {
    let down = true;
    const respond = () => (down ? jsonResponse({}, 500) : jsonResponse(systemInfo({ canTurnOff: true })));
    const { user } = renderApp("/actions", { fetch: systemFetch({ "/api/system/info": respond }) });
    const actions = within(screen.getByRole("dialog", { name: "Actions" }));
    expect(await actions.findByRole("alert")).toHaveTextContent("Could not load system information");
    down = false;
    await user.click(actions.getByRole("button", { name: "Retry" }));
    expect(await actions.findByRole("button", { name: "Turn off Raspberry Pi" })).toBeInTheDocument();
  });

  it("explains an unknown address", () => {
    renderApp("/system/nope", { fetch: systemFetch() });
    expect(screen.getByRole("dialog", { name: "Not found" })).toHaveTextContent("There is no screen at this address.");
  });
});
