import { screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fakeFetch, startupState } from "../test/fakeFetch";
import { currentPath, renderApp } from "../test/render";

const nav = () => screen.getByRole("navigation", { name: "Primary" });
const menu = (name: string) => within(nav()).getByRole("button", { name });

const pick = async (user: ReturnType<typeof renderApp>["user"], menuName: string, item: string) => {
  await user.click(menu(menuName));
  await user.click(screen.getByRole("menuitem", { name: item }));
};

afterEach(() => localStorage.clear());

describe("App shell", () => {
  it("renders the control screen with the viewer at the index route", () => {
    renderApp("/");
    expect(screen.getByRole("heading", { name: "Control" })).toBeInTheDocument();
    expect(screen.getByRole("figure", { name: "Gcode and board viewer" })).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("has the classic menus in the classic order", () => {
    renderApp("/");
    expect(
      within(nav())
        .getAllByRole("button")
        .map((button) => button.textContent),
    ).toEqual(["G-Code ▾", "Actions", "Settings ▾", "Boards ▾", "Help ▾"]);
  });

  it.each([
    ["G-Code", "Open", "/files/open", "Open G-Code"],
    ["G-Code", "Save", "/files/save", "Save G-Code"],
    ["G-Code", "Upload", "/files/upload", "Upload G-Code"],
    ["G-Code", "View/Edit", "/files/edit", "View/Edit G-Code"],
    ["G-Code", "Send Custom GCode", "/files/send", "Send Custom G-Code"],
    ["Settings", "Maslow", "/settings", "Settings"],
    ["Settings", "Advanced", "/settings/advanced", "Settings"],
    ["Settings", "WebControl", "/settings/webcontrol", "Settings"],
    ["Settings", "Camera", "/settings/camera", "Settings"],
    ["Settings", "GPIO", "/settings/gpio", "Settings"],
    ["Boards", "Open Board", "/boards/open", "Open Board"],
    ["Boards", "Save Board", "/boards/save", "Save Board"],
    ["Boards", "Create/Edit Board", "/boards/edit", "Create/Edit Board"],
    ["Boards", "Trim Board", "/boards/trim", "Trim Board"],
    ["Help", "Getting Started", "/machine/getting-started", "Getting Started"],
    ["Help", "Offline Help", "/help", "Help"],
    ["Help", "About", "/about", "About"],
    ["Help", "Update", "/updates", "Update"],
  ])("%s ▸ %s opens %s over the control screen", async (menuName, item, path, title) => {
    const { user } = renderApp("/");
    await pick(user, menuName, item);
    expect(currentPath()).toBe(path);
    expect(screen.getByRole("dialog", { name: title })).toBeInTheDocument();
    expect(screen.getByRole("figure", { name: "Gcode and board viewer", hidden: true })).toBeInTheDocument();

    await user.click(within(screen.getByRole("dialog", { name: title })).getByRole("button", { name: "Close" }));
    expect(currentPath()).toBe("/");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("opens the Actions modal straight from the menu bar", async () => {
    const { user } = renderApp("/");
    await user.click(menu("Actions"));
    expect(screen.getByRole("dialog", { name: "Actions" })).toBeInTheDocument();
  });

  it("links Online Help and Logs out to new tabs", async () => {
    const { user } = renderApp("/");
    await user.click(menu("Help"));
    expect(screen.getByRole("menuitem", { name: "Online Help" })).toHaveAttribute(
      "href",
      "https://webcontrolcnc.github.io/WebControl/",
    );
    expect(screen.getByRole("menuitem", { name: "Logs" })).toHaveAttribute("href", "/next/logs");
    expect(screen.getByRole("menuitem", { name: "Logs" })).toHaveAttribute("target", "_blank");
    await user.click(screen.getByRole("menuitem", { name: "Logs" }));
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("runs Process GCode and Clear Board at once", async () => {
    const { user, socket } = renderApp("/");
    await pick(user, "Boards", "Process GCode");
    await pick(user, "Boards", "Clear Board");
    expect(socket.actions()).toEqual([
      ["boardProcessGCode", "", ""],
      ["boardClearBoard", "", ""],
    ]);
    expect(currentPath()).toBe("/");
  });

  it("clears the gcode after a confirm, and not on cancel", async () => {
    const { user, socket } = renderApp("/");
    await pick(user, "G-Code", "Clear GCode");
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(socket.actions()).toEqual([]);

    await pick(user, "G-Code", "Clear GCode");
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Clear GCode" }));
    expect(socket.actions()).toEqual([["clearGCode", "", ""]]);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("disables loading and clearing gcode while a job is in progress, saying why", async () => {
    const { user, store } = renderApp("/", {
      fetch: fakeFetch({ "GET /api/state": startupState({ jobInProgress: true }) }),
    });
    await waitFor(() => expect(store.getState().job.inProgress).toBe(true));
    await user.click(menu("G-Code"));
    ["Open", "Upload", "Clear GCode"].forEach((name) => {
      const item = screen.getByRole("menuitem", { name: new RegExp(`^${name}`) });
      expect(item).toHaveAttribute("aria-disabled", "true");
      expect(item).toHaveTextContent("Stop the job first");
    });
    await user.click(screen.getByRole("menuitem", { name: /^Open/ }));
    expect(currentPath()).toBe("/");
    expect(screen.getByRole("menuitem", { name: "Save" })).not.toHaveAttribute("aria-disabled");
  });

  it("closes a modal back past the screens opened inside it", async () => {
    const { user } = renderApp("/", { fetch: fakeFetch({ "/api/machine/setup": () => new Promise(() => {}) }) });
    await pick(user, "Help", "Getting Started");
    await user.click(screen.getByRole("link", { name: "Quick configure" }));
    expect(currentPath()).toBe("/machine/quick-configure");
    await user.click(
      within(screen.getByRole("dialog", { name: "Quick Configure" })).getByRole("button", { name: "Close" }),
    );
    await waitFor(() => expect(currentPath()).toBe("/"));
  });

  it("closes a modal switched between settings tabs in one step", async () => {
    const { user } = renderApp("/");
    await pick(user, "Settings", "Maslow");
    await user.click(screen.getByRole("link", { name: "GPIO" }));
    expect(currentPath()).toBe("/settings/gpio");
    await user.keyboard("{Escape}");
    expect(currentPath()).toBe("/");
  });

  it("closes a deep-linked modal to the control screen", async () => {
    const { user } = renderApp("/about");
    await user.click(within(screen.getByRole("dialog", { name: "About" })).getByRole("button", { name: "Close" }));
    expect(currentPath()).toBe("/");
  });

  it("puts a Stop button on every route modal", async () => {
    const { user, socket } = renderApp("/settings");
    await user.click(screen.getByRole("button", { name: "Stop machine" }));
    expect(socket.actions()).toEqual([["stopRun", "", ""]]);
  });

  it("returns home from the brand link", async () => {
    const { user } = renderApp("/calibration");
    await user.click(screen.getByRole("link", { name: "← Back to Control" }));
    expect(currentPath()).toBe("/");
  });

  it("toggles the phone menu and closes it after choosing", async () => {
    const { user } = renderApp("/");
    const toggle = screen.getByRole("button", { name: "Menu" });
    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(nav()).toHaveAttribute("data-open", "true");
    await pick(user, "Boards", "Trim Board");
    expect(toggle).toHaveAttribute("aria-expanded", "false");
  });
});

describe("menus", () => {
  it("moves with the arrow keys, Home and End, and closes on Escape", async () => {
    const { user } = renderApp("/");
    menu("Boards").focus();
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: "Open Board" })).toHaveFocus();
    await user.keyboard("{ArrowUp}");
    expect(screen.getByRole("menuitem", { name: "Clear Board" })).toHaveFocus();
    await user.keyboard("{Home}");
    expect(screen.getByRole("menuitem", { name: "Open Board" })).toHaveFocus();
    await user.keyboard("{End}{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: "Open Board" })).toHaveFocus();
    await user.keyboard("a");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(menu("Boards")).toHaveFocus();
  });

  it("closes on Tab, on a click elsewhere, and on a second click", async () => {
    const { user } = renderApp("/");
    await user.click(menu("Help"));
    await user.tab();
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();

    await user.click(menu("Help"));
    await user.click(screen.getByRole("menuitem", { name: "About" }).closest("ul")!);
    expect(screen.getByRole("menu")).toBeInTheDocument();
    await user.click(screen.getByRole("heading", { name: "Controls" }));
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();

    await user.click(menu("Help"));
    await user.click(menu("Help"));
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    menu("Help").focus();
    await user.keyboard("{ArrowUp}");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });
});

describe("theme", () => {
  const theme = () => document.documentElement.getAttribute("data-theme");

  it("cycles system, light and dark, and remembers the choice", async () => {
    const { user, unmount } = renderApp("/");
    const toggle = screen.getByRole("button", { name: "Theme: System" });
    expect(theme()).toBeNull();
    await user.click(toggle);
    expect(theme()).toBe("light");
    await user.click(toggle);
    expect(theme()).toBe("dark");
    expect(localStorage.getItem("webcontrol.theme")).toBe("dark");
    unmount();

    renderApp("/");
    expect(screen.getByRole("button", { name: "Theme: Dark" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Theme: Dark" }));
    expect(theme()).toBeNull();
    expect(localStorage.getItem("webcontrol.theme")).toBeNull();
  });

  it("follows the system preference and survives storage that throws", async () => {
    const listeners: (() => void)[] = [];
    const query = {
      matches: true,
      addEventListener: (_: string, l: () => void) => listeners.push(l),
      removeEventListener: vi.fn(),
    };
    vi.stubGlobal("matchMedia", () => query);
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    const { user } = renderApp("/");
    expect(screen.getByRole("button", { name: "Theme: System" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Theme: System" }));
    expect(theme()).toBe("light");
    expect(listeners.length).toBeGreaterThan(0);
  });
});
