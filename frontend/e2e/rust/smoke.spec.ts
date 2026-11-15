import { expect, test } from "@playwright/test";
import { waitForServer } from "../helpers";

test("the same build connects to the Rust backend over plain WebSockets", async ({ page }) => {
  const sockets: string[] = [];
  page.on("websocket", (socket) => sockets.push(socket.url()));
  const polls: string[] = [];
  page.on("request", (request) => {
    if (request.url().includes("/socket.io/")) polls.push(request.url());
  });

  await page.goto("/");
  await expect(page).toHaveURL(/\/next\/$/);
  await waitForServer(page);
  await expect(page.getByText("Controller: not connected")).toBeVisible();
  await expect(page.getByText("Firmware: unknown")).toBeVisible();
  expect(sockets.some((url) => url.endsWith("/ws?ns=%2FMaslowCNC"))).toBe(true);
  expect(polls).toEqual([]);
});

test("loading the control screen raises no alert", async ({ page }) => {
  await page.goto("/next/");
  await waitForServer(page);
  await expect(page.getByText("Controller: not connected")).toBeVisible();
  await expect(page.getByRole("alertdialog")).toHaveCount(0);
});

test("the log viewer opens its own WebSocket", async ({ page }) => {
  const opened = page.waitForEvent("websocket", (socket) => socket.url().endsWith("/ws?ns=%2FMaslowCNCLogs"));
  await page.goto("/next/logs");
  await opened;
});
