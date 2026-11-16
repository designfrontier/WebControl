import { expect, test } from "@playwright/test";
import type { APIRequestContext } from "@playwright/test";
import { openActions } from "./helpers";

const setOptical = async (request: APIRequestContext, on: boolean) =>
  expect(
    await request.put("/api/settings/webcontrol", { data: { values: { opticalCalibrationEnabled: on } } }),
  ).toBeOK();

test.beforeEach(({ request }) => setOptical(request, false));
test.afterEach(({ request }) => setOptical(request, false));

test("optical calibration stays hidden until its setting is saved, then appears without a reload", async ({ page }) => {
  await page.goto("/calibration");
  const methods = page.getByRole("navigation", { name: "Calibration methods" });
  await expect(methods.getByRole("link")).toHaveText(["Triangular", "Holey"]);

  await page.goto("/settings/webcontrol");
  await page.evaluate(() => Object.assign(window, { opticalMark: true }));
  const form = page.getByRole("form", { name: "WebControl Settings" });
  await form.getByLabel("Optical calibration (experimental)").check();
  await form.getByRole("button", { name: "Save" }).click();
  await expect(form.getByText("WebControl Settings saved.")).toBeVisible();

  const settings = page.getByRole("dialog", { name: "Settings" });
  await settings.getByRole("button", { name: "Close", exact: true }).click();
  await expect(settings).toBeHidden();
  await openActions(page);
  const actions = page.getByRole("dialog", { name: "Actions" });
  await actions.getByRole("link", { name: "Optical calibration (experimental)" }).click();
  await expect(page).toHaveURL(/\/calibration\/optical$/);
  await expect(methods.getByRole("link", { name: "Optical" })).toHaveAttribute("aria-current", "page");
  expect(await page.evaluate(() => "opticalMark" in window)).toBe(true);

  await expect(page.getByRole("heading", { name: "Optical calibration" })).toBeVisible();
  await expect(page.getByRole("note", { name: "Experimental" })).toContainText(
    "The measurement alone corrects nothing.",
  );
  await expect(page.getByRole("link", { name: "Printable target (A4)" })).toHaveAttribute(
    "href",
    "/api/calibration/optical/target.pdf?paper=a4",
  );
  // Until the scan backend lands the state route answers 501, which the screen reports.
  await expect(
    page.getByRole("form", { name: "Optical calibration settings" }).or(page.getByRole("alert")),
  ).toBeVisible();
});

test("its address explains how to turn it on while the setting is off", async ({ page }) => {
  await page.goto("/calibration/optical");
  await expect(page.getByRole("alert")).toContainText("Optical calibration is turned off.");
  await expect(page.getByRole("navigation", { name: "Calibration methods" }).getByRole("link")).toHaveText([
    "Triangular",
    "Holey",
  ]);
});
