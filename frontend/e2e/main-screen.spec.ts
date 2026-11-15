import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";
import { closeServerDialogs, resetMachine, waitForServer } from "./helpers";

// These tests jog, start runs and write units: safe only because the e2e backend runs on a temporary home with no port.
const serverDialog = (page: Page) => page.locator("dialog[open][data-kind]");

test.beforeEach(async ({ page, baseURL }) => {
  await resetMachine(baseURL!);
  await page.goto("/");
  await waitForServer(page);
  await expect(page.getByRole("button", { name: "Play" })).toBeEnabled({ timeout: 15_000 });
});

test("rapid server alerts show one dialog at a time and leave the controls usable", async ({ page }) => {
  const play = page.getByRole("button", { name: "Play" });
  await play.click();
  await expect(serverDialog(page)).toBeVisible();
  await play.click({ force: true });
  await play.click({ force: true });

  await expect(page.locator("dialog[open]")).toHaveCount(1);

  const seen = await closeServerDialogs(page);
  expect(seen.join("\n")).toContain("No GCode file loaded.");
  await expect(page.locator("dialog[open]")).toHaveCount(0);

  // A stuck overlay would intercept this click and fail it.
  await page.getByRole("button", { name: "Up", exact: true }).click();
  await expect(serverDialog(page)).toHaveCount(0);
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
  const names = ["Home", "Set Home Here", "Play", "Stop", "Go to line", "Camera"];
  await Promise.all(
    names.map((name) => expect(page.getByRole("button", { name, exact: true })).toHaveText(new RegExp(name))),
  );
  await expect(page.getByRole("link", { name: "Z-Axis" })).toBeVisible();
  await expect(page.getByText("Job", { exact: true })).toBeVisible();
});

test("the Z-Axis button opens its modal, which moves Z and closes again", async ({ page }) => {
  await page.getByRole("link", { name: "Z-Axis" }).click();
  const zAxis = page.getByRole("dialog", { name: "Z-Axis" });
  await expect(zAxis.getByRole("button", { name: "Stop Z" })).toBeVisible();
  await zAxis.getByRole("button", { name: /Raise/ }).click();
  await page.keyboard.press("Escape");
  await expect(zAxis).toBeHidden();
  await expect(page).toHaveURL(/\/$/);
});
