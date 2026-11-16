import { act, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fakeFetch, startupState } from "../test/fakeFetch";
import { renderApp, renderWithProviders } from "../test/render";
import { ThemeProvider } from "./theme";
import { useTheme } from "./themePreference";

const theme = () => document.documentElement.getAttribute("data-theme");
const LEGACY_KEY = "webcontrol.theme";

afterEach(() => {
  localStorage.clear();
  document.documentElement.removeAttribute("data-theme");
});

describe("theme", () => {
  it("shows the server's theme, with no theme control in the header", async () => {
    renderApp("/", { fetch: fakeFetch({ "GET /api/state": startupState({ theme: "dark" }) }) });
    await waitFor(() => expect(theme()).toBe("dark"));
    expect(within(screen.getByRole("banner")).queryByRole("button", { name: /theme/i })).not.toBeInTheDocument();
  });

  it("switches live when the server pushes a new theme", async () => {
    const { socket, store } = renderApp("/", {
      fetch: fakeFetch({ "GET /api/state": startupState({ theme: "light" }) }),
    });
    await waitFor(() => expect(theme()).toBe("light"));
    socket.serverMessage("theme", { theme: "dark" });
    expect(theme()).toBe("dark");
    socket.serverMessage("theme", { theme: "system" });
    expect(theme()).toBeNull();
    expect(store.getState().theme).toBe("system");
  });

  it("shows an older build's per-browser choice only until the server's theme arrives, then forgets it", async () => {
    localStorage.setItem(LEGACY_KEY, "dark");
    renderApp("/", { fetch: fakeFetch({ "GET /api/state": startupState({ theme: "system" }) }) });
    expect(theme()).toBe("dark");
    await waitFor(() => expect(theme()).toBeNull());
    expect(localStorage.getItem(LEGACY_KEY)).toBeNull();
  });

  it("keeps the per-browser choice while the server names no theme", async () => {
    localStorage.setItem(LEGACY_KEY, "light");
    const { store } = renderApp("/");
    await waitFor(() => expect(store.getState().hostAddress).toBe("192.168.1.20:5000"));
    expect(theme()).toBe("light");
    expect(localStorage.getItem(LEGACY_KEY)).toBe("light");
  });

  it("applies the server's theme when the browser blocks storage", async () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "removeItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    renderApp("/", { fetch: fakeFetch({ "GET /api/state": startupState({ theme: "light" }) }) });
    expect(theme()).toBeNull();
    await waitFor(() => expect(theme()).toBe("light"));
  });

  it("resolves System to the device's colour scheme and follows its changes", () => {
    const listeners: (() => void)[] = [];
    const query = {
      matches: true,
      addEventListener: (_: string, listener: () => void) => listeners.push(listener),
      removeEventListener: vi.fn(),
    };
    vi.stubGlobal("matchMedia", () => query);
    const Probe = () => <p>{useTheme().theme}</p>;
    renderWithProviders(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
      { machine: { theme: "system" } },
    );
    expect(screen.getByText("dark")).toBeInTheDocument();
    query.matches = false;
    act(() => listeners.forEach((listener) => listener()));
    expect(screen.getByText("light")).toBeInTheDocument();
  });
});
