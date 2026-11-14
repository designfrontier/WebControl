import { screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { jsonResponse } from "../../test/fakeFetch";
import { currentPath, renderApp } from "../../test/render";
import { systemFetch, systemInfo } from "../../test/systemFixtures";

vi.mock("socket.io-client", () => ({
  io: () => ({ id: undefined, connected: false, on: vi.fn(), disconnect: vi.fn() }),
}));

const subnav = () => within(screen.getByRole("navigation", { name: "System" }));

describe("SystemPage", () => {
  it("has a tab for each classic system screen", async () => {
    const { user } = renderApp("/system", { fetch: systemFetch() });
    expect(screen.getByRole("heading", { name: "System", level: 1 })).toBeInTheDocument();
    expect(subnav().getByRole("link", { name: "Actions" })).toHaveAttribute("aria-current", "page");

    await user.click(subnav().getByRole("link", { name: "Logs" }));
    expect(currentPath()).toBe("/system/logs");
    expect(screen.getByRole("heading", { name: "Logs", level: 2 })).toBeInTheDocument();

    await user.click(subnav().getByRole("link", { name: "Help" }));
    expect(screen.getByRole("heading", { name: "Help", level: 2 })).toBeInTheDocument();
  });

  it("shows version and firmware on About", async () => {
    renderApp("/system/about", { fetch: systemFetch() });
    expect(await screen.findByText("v0.95")).toBeInTheDocument();
    expect(screen.getByText("RPI")).toBeInTheDocument();
    expect(screen.getByText("1.26")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /github\.com\/WebControlCNC/ })).toHaveAttribute("target", "_blank");
  });

  it("shows placeholders on About until info loads, and unreported firmware", async () => {
    const pending = renderApp("/system/about", {
      fetch: systemFetch({ "/api/system/info": () => new Promise(() => {}) }),
    });
    expect(screen.getAllByText("…")).toHaveLength(3);
    pending.unmount();

    renderApp("/system/about", {
      fetch: systemFetch({ "/api/system/info": systemInfo({ controllerFirmwareVersion: 0 }) }),
    });
    expect(await screen.findByText("Not reported")).toBeInTheDocument();
  });

  it("badges Updates when the server reports an update", async () => {
    renderApp("/system", {
      fetch: systemFetch({ "/api/system/info": systemInfo({ update: { available: true, version: "v0.96" } }) }),
    });
    expect(await subnav().findByRole("link", { name: "Updates (update available)" })).toBeInTheDocument();
  });

  it("badges Updates when pyinstallUpdate arrives, and shows it on the Updates tab", async () => {
    const { socket, user } = renderApp("/system", { fetch: systemFetch() });
    expect(subnav().getByRole("link", { name: "Updates" })).toBeInTheDocument();
    socket.serverMessage("pyinstallUpdate", "on");
    await user.click(subnav().getByRole("link", { name: "Updates (update available)" }));
    expect(screen.getByText("An update is available")).toBeInTheDocument();
  });

  it("retries when system info fails", async () => {
    let down = true;
    const respond = () => (down ? jsonResponse({}, 500) : jsonResponse(systemInfo({ canTurnOff: true })));
    const { user } = renderApp("/system", { fetch: systemFetch({ "/api/system/info": respond }) });
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not load system information");
    down = false;
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("button", { name: "Turn off Raspberry Pi" })).toBeInTheDocument();
  });

  it("has a not-found screen for unknown system paths", () => {
    renderApp("/system/nope", { fetch: systemFetch() });
    expect(screen.getByText("There is no system screen at this address.")).toBeInTheDocument();
  });
});
