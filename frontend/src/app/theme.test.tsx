import { act, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fakeFetch, startupState } from "../test/fakeFetch";
import { renderApp, renderWithProviders } from "../test/render";
import { ThemeProvider } from "./theme";
import { useTheme } from "./themePreference";

const theme = () => document.documentElement.getAttribute("data-theme");
const STORAGE_KEY = "webcontrol.theme";

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

  it("starts a load with the theme this browser last saw, before the server answers", async () => {
    const first = renderApp("/", { fetch: fakeFetch({ "GET /api/state": startupState({ theme: "dark" }) }) });
    await waitFor(() => expect(localStorage.getItem(STORAGE_KEY)).toBe("dark"));
    first.unmount();
    document.documentElement.removeAttribute("data-theme");

    renderApp("/", { fetch: fakeFetch({ "GET /api/state": () => new Promise<Response>(() => {}) }) });
    expect(theme()).toBe("dark");
  });

  it("replaces a stored choice with the server's theme once it arrives", async () => {
    localStorage.setItem(STORAGE_KEY, "dark");
    const { socket } = renderApp("/", { fetch: fakeFetch({ "GET /api/state": startupState({ theme: "system" }) }) });
    expect(theme()).toBe("dark");
    await waitFor(() => expect(theme()).toBeNull());
    expect(localStorage.getItem(STORAGE_KEY)).toBeNull();
    socket.serverMessage("theme", { theme: "light" });
    expect(localStorage.getItem(STORAGE_KEY)).toBe("light");
  });

  it("keeps the per-browser choice while the server names no theme", async () => {
    localStorage.setItem(STORAGE_KEY, "light");
    const { store } = renderApp("/");
    await waitFor(() => expect(store.getState().hostAddress).toBe("192.168.1.20:5000"));
    expect(theme()).toBe("light");
    expect(localStorage.getItem(STORAGE_KEY)).toBe("light");
  });

  it("applies the server's theme when the browser blocks storage", async () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    renderApp("/", { fetch: fakeFetch({ "GET /api/state": startupState({ theme: "light" }) }) });
    expect(theme()).toBeNull();
    await waitFor(() => expect(theme()).toBe("light"));
  });

  it("colours the browser chrome after the applied theme, following the device under System", async () => {
    const themeColor = () => document.head.querySelector('meta[name="theme-color"]')?.getAttribute("content");
    const listeners: (() => void)[] = [];
    const query = { matches: false, addEventListener: (_: string, on: () => void) => listeners.push(on) };
    vi.stubGlobal("matchMedia", () => ({ ...query, removeEventListener: vi.fn() }));
    const { socket } = renderApp("/", { fetch: fakeFetch({ "GET /api/state": startupState({ theme: "dark" }) }) });
    await waitFor(() => expect(themeColor()).toBe("#14171c"));
    socket.serverMessage("theme", { theme: "light" });
    expect(themeColor()).toBe("#ffffff");
    socket.serverMessage("theme", { theme: "system" });
    expect(themeColor()).toBe("#ffffff");
    query.matches = true;
    act(() => listeners.forEach((listener) => listener()));
    expect(themeColor()).toBe("#14171c");
    expect(document.head.querySelectorAll('meta[name="theme-color"]')).toHaveLength(1);
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
