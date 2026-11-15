import { expect } from "@playwright/test";
import type { Page } from "@playwright/test";

/** A test's own connection to /MaslowCNC. */
export type BackendSocket = { emit: (event: string, data: unknown) => void; disconnect: () => void };

const NAMESPACE = "/MaslowCNC";

/** The plain-WebSocket transport (contract/README.md): ready once the server's connect frame arrives. */
export const connectBackend = (baseURL: string) =>
  new Promise<BackendSocket>((resolve, reject) => {
    const socket = new WebSocket(`${baseURL.replace(/^http/, "ws")}/ws?ns=${encodeURIComponent(NAMESPACE)}`);
    const backend: BackendSocket = {
      emit: (event, data) => socket.send(JSON.stringify({ ns: NAMESPACE, event, data })),
      disconnect: () => socket.close(),
    };
    socket.addEventListener("message", (message) => {
      if ((JSON.parse(String(message.data)) as { event: string }).event === "connect") resolve(backend);
    });
    socket.addEventListener("error", () => reject(new Error(`WebSocket to ${baseURL} failed`)));
  });

export const waitForServer = (page: Page) =>
  expect(page.getByText(/^Server: connected/)).toBeVisible({ timeout: 15_000 });

const openDrawer = async (page: Page) => {
  const toggle = page.getByRole("button", { name: "Menu", exact: true });
  if ((await toggle.isVisible()) && (await toggle.getAttribute("aria-expanded")) !== "true") await toggle.click();
};

/** Picks a top-bar menu item; on phones the menus sit behind the Menu toggle. */
export const chooseMenu = async (page: Page, menu: string, item: string) => {
  await openDrawer(page);
  await page.getByRole("navigation", { name: "Primary" }).getByRole("button", { name: menu }).click();
  await page.getByRole("menuitem", { name: item, exact: true }).click();
};

export const openActions = async (page: Page) => {
  await openDrawer(page);
  await page.getByRole("navigation", { name: "Primary" }).getByRole("button", { name: "Actions" }).click();
};

/** Closes server dialogs one by one, returning every message seen. */
export const closeServerDialogs = async (page: Page, seen: string[] = []): Promise<string[]> => {
  const dialog = page.locator("dialog[open][data-kind]");
  if ((await dialog.count()) === 0) return seen;
  const text = (await dialog.textContent()) ?? "";
  await dialog.getByRole("button", { name: "Close", exact: true }).click();
  return closeServerDialogs(page, [...seen, text]);
};

const pause = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/** Stops any job and unloads the gcode, so Play reaches the "No GCode file loaded." alert. */
export const resetMachine = async (baseURL: string) => {
  const socket = await connectBackend(baseURL);
  const action = (command: string) => socket.emit("action", { data: { command, arg: "", arg1: "" } });
  action("stopRun");
  await pause(300);
  action("clearGCode");
  await pause(300);
  socket.disconnect();
};
