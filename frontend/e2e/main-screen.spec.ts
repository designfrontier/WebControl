import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";

const openDialog = (page: Page) => page.getByRole("dialog").or(page.getByRole("alertdialog"));

/** Closes server dialogs one by one, returning every message seen; a first-run notice may be queued too. */
const closeAllDialogs = async (page: Page, seen: string[] = []): Promise<string[]> => {
  if ((await openDialog(page).count()) === 0) return seen;
  const dialog = openDialog(page);
  const text = (await dialog.textContent()) ?? "";
  await dialog.getByRole("button", { name: "Close" }).click();
  return closeAllDialogs(page, [...seen, text]);
};

test.beforeEach(async ({ page }) => {
  await page.goto("/next/");
  await expect(page.getByText(/^Server: connected/)).toBeVisible({ timeout: 15_000 });
  await expect(page.getByRole("button", { name: "Play" })).toBeEnabled({ timeout: 15_000 });
});

test("rapid server alerts show one dialog at a time and leave the controls usable", async ({ page }) => {
  const play = page.getByRole("button", { name: "Play" });
  await play.click();
  await expect(openDialog(page)).toBeVisible();
  await play.click({ force: true });
  await play.click({ force: true });

  await expect(page.locator(".dialog-backdrop")).toHaveCount(1);
  await expect(openDialog(page)).toHaveCount(1);

  const seen = await closeAllDialogs(page);
  expect(seen.join("\n")).toContain("No GCode file loaded.");
  await expect(page.locator(".dialog-backdrop")).toHaveCount(0);

  // A stuck overlay would intercept this click and fail it.
  await page.getByRole("button", { name: "Up", exact: true }).click();
  await expect(openDialog(page)).toHaveCount(0);
});

test("switching units round-trips through the server", async ({ page }) => {
  const units = page.getByRole("button", { name: /^Units: (mm|in)$/ });
  const before = await units.textContent();
  const after = before === "Units: mm" ? "Units: in" : "Units: mm";

  await units.click();
  await expect(page.getByRole("button", { name: after })).toBeVisible();
  await page.getByRole("button", { name: after }).click();
  await expect(page.getByRole("button", { name: before! })).toBeVisible();
});

test("labels every control with visible text", async ({ page }) => {
  const names = ["Home", "Set Home Here", "Return to Center", "Raise", "Lower", "Stop", "Stop Z", "Next line"];
  await Promise.all(
    names.map((name) => expect(page.getByRole("button", { name, exact: true })).toHaveText(new RegExp(name))),
  );
});
