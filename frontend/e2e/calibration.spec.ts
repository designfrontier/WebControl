import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";

const status = (page: Page) => page.getByRole("status", { name: "Calibration status" });
const row = (page: Page, name: string) => page.getByRole("row").filter({ has: page.getByRole("rowheader", { name }) });

test("triangular calibration calculates on the server and accepts the result", async ({ page }) => {
  await page.goto("/next/calibration");
  await page.getByRole("link", { name: "Open triangular calibration" }).click();
  await expect(page).toHaveURL(/\/next\/calibration\/triangular$/);
  await expect(page.getByRole("img", { name: /Test pattern/ })).toBeVisible();
  await expect(row(page, "Height of motors").getByRole("cell").first()).toHaveText(/^\d/);

  const bit = page.getByRole("textbox", { name: "Router bit diameter" });
  await bit.fill("30");
  await page.getByRole("button", { name: "Calculate" }).click();
  await expect(status(page)).toHaveText(/bit diameter of 30.0 mm seems wrong/);

  await bit.fill("6.35");
  await page.getByRole("button", { name: "Calculate" }).click();
  await expect(status(page)).toHaveText(/Review the calculated values/, { timeout: 15_000 });
  await expect(row(page, "Calibration error").getByRole("cell").last()).toHaveText(/^-?\d/);

  await page.getByRole("button", { name: "Accept results" }).click();
  await expect(status(page)).toHaveText(/Accepted/);
  await expect(page.getByRole("button", { name: "Accept results" })).toBeDisabled();
});

test("optical calibration loads, validates and saves its settings", async ({ page }) => {
  await page.goto("/next/calibration/optical");
  const form = page.getByRole("form", { name: "Optical calibration settings" });
  await expect(form).toBeVisible();
  await page.getByText("Curve fitting", { exact: true }).click();
  await expect(page.getByRole("img", { name: "X curve: 15 lines" })).toBeVisible();

  const tolerance = page.getByRole("textbox", { name: "Position tolerance" });
  await tolerance.fill("abc");
  await page.getByRole("button", { name: "Save settings" }).click();
  await expect(status(page)).toHaveText("Check these settings: Position tolerance.");

  await tolerance.fill("0.2");
  await page.getByRole("button", { name: "Save settings" }).click();
  await expect(status(page)).toHaveText("Settings saved.");

  await page.reload();
  await expect(page.getByRole("textbox", { name: "Position tolerance" })).toHaveValue("0.2");
});
