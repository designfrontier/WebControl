import { expect } from "@playwright/test";
import type { Page } from "@playwright/test";
import { io } from "socket.io-client";

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
  const socket = io(`${baseURL}/MaslowCNC`, { transports: ["polling"] });
  await new Promise((resolve) => socket.on("connect", () => resolve(null)));
  const action = (command: string) => socket.emit("action", { data: { command, arg: "", arg1: "" } });
  action("stopRun");
  await pause(300);
  action("clearGCode");
  await pause(300);
  socket.disconnect();
};
