import { expect, test } from "@playwright/test";
import { waitForServer } from "./helpers";

test("loads the shell from the Flask server and connects its socket", async ({ page }) => {
  await page.goto("/next/");

  await expect(page.getByRole("link", { name: "WebControl" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Controls" })).toBeVisible();
  await waitForServer(page);
  await expect(page.getByText(/^Controller: (not connected|unknown)/)).toBeVisible();
  await expect(page.getByText(/^Firmware: unknown$|^(Stock|Holey|Custom) \d/)).toBeVisible();
});

test("an old section address opens its modal over the control screen", async ({ page }) => {
  await page.goto("/next/system/about");
  await expect(page).toHaveURL(/\/next\/about$/);
  await expect(page.getByRole("dialog", { name: "About" })).toBeVisible();
  await page.getByRole("dialog", { name: "About" }).getByRole("button", { name: "Close" }).click();
  await expect(page).toHaveURL(/\/next\/?$/);
  await expect(page.getByRole("button", { name: "Play" })).toBeVisible();
});
