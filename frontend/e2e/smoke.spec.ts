import { expect, test } from "@playwright/test";
import { waitForServer } from "./helpers";

test("loads the shell at / and connects over plain WebSockets", async ({ page }) => {
  const sockets: string[] = [];
  page.on("websocket", (socket) => sockets.push(socket.url()));
  await page.goto("/");

  await expect(page.getByRole("link", { name: "WebControl" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Controls" })).toBeVisible();
  await waitForServer(page);
  await expect(page.getByText("Controller: not connected")).toBeVisible();
  await expect(page.getByText("Firmware: unknown")).toBeVisible();
  await expect(page.getByRole("alertdialog")).toHaveCount(0);
  expect(sockets.some((url) => url.endsWith("/ws?ns=%2FMaslowCNC"))).toBe(true);
});

test("an old /next address redirects to the same screen at the root", async ({ page }) => {
  await page.goto("/next/settings/advanced?tab=1");
  await expect(page).toHaveURL(/:\d+\/settings\/advanced\?tab=1$/);
  await expect(page.getByRole("dialog", { name: "Settings" })).toBeVisible();
});

test("an old section address opens its modal over the control screen", async ({ page }) => {
  await page.goto("/system/about");
  await expect(page).toHaveURL(/\/about$/);
  await expect(page.getByRole("dialog", { name: "About" })).toBeVisible();
  await page.getByRole("dialog", { name: "About" }).getByRole("button", { name: "Close" }).click();
  await expect(page).toHaveURL(/\/$/);
  await expect(page.getByRole("button", { name: "Play" })).toBeVisible();
});

test("the log viewer opens its own WebSocket", async ({ page }) => {
  const opened = page.waitForEvent("websocket", (socket) => socket.url().endsWith("/ws?ns=%2FMaslowCNCLogs"));
  await page.goto("/logs");
  await opened;
});
