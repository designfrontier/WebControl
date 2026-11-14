import { expect, test } from "@playwright/test";

test("loads the shell from the Flask server and connects its socket", async ({ page }) => {
  await page.goto("/next/");

  await expect(page.getByRole("link", { name: "WebControl" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Control" })).toBeVisible();
  await expect(page.getByText(/^Server: connected/)).toBeVisible({ timeout: 15_000 });
  await expect(page.getByText(/^Controller: (not connected|unknown)/)).toBeVisible();
});

test("deep links load through the SPA fallback and the nav reaches every section", async ({ page, isMobile }) => {
  await page.goto("/next/settings");
  await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();

  if (isMobile) await page.getByRole("button", { name: "Menu" }).click();
  await page.getByRole("navigation", { name: "Primary" }).getByRole("link", { name: "Calibration" }).click();

  await expect(page).toHaveURL(/\/next\/calibration$/);
  await expect(page.getByRole("heading", { name: "Calibration" })).toBeVisible();
});
