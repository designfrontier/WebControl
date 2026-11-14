import { expect, test } from "@playwright/test";

test("quick configure saves to the server and reloads", async ({ page }) => {
  await page.goto("/next/machine");
  await expect(page.getByRole("heading", { level: 1, name: "Machine" })).toBeVisible();
  await page.getByRole("link", { name: "Quick configure" }).click();

  const kit = page.getByRole("combobox", { name: "Kit" });
  await expect(kit).toBeVisible();
  const feeds = page.getByRole("combobox", { name: "Chain feeds off the" });
  const height = page.getByRole("textbox", { name: "Height of motors above workspace (mm)" });
  const radius = page.getByRole("textbox", { name: "Rotation radius (mm)" });
  const original = {
    kit: await kit.inputValue(),
    radius: await radius.inputValue(),
    feeds: await feeds.inputValue(),
    spacing: await page.getByRole("textbox", { name: "Distance between motors (mm)" }).inputValue(),
    height: await height.inputValue(),
  };
  await kit.selectOption("Quadrilateral");
  await expect(radius).toHaveValue("0");
  await page.getByRole("combobox", { name: "Chain feeds off the" }).selectOption("Bottom");
  const spacing = page.getByRole("textbox", { name: "Distance between motors (mm)" });
  await spacing.fill("3000.4");
  await page.getByRole("button", { name: "Subtract 40.4 mm" }).click();
  await expect(spacing).toHaveValue("2960");
  await page.getByRole("textbox", { name: "Height of motors above workspace (mm)" }).fill("480");
  await page.getByRole("button", { name: "Save configuration" }).click();
  await expect(page.getByText("Saved. The controller has the new machine dimensions.")).toBeVisible();

  await page.reload();
  await expect(kit).toHaveValue("Quadrilateral");
  await expect(page.getByRole("combobox", { name: "Chain feeds off the" })).toHaveValue("Bottom");
  await expect(spacing).toHaveValue("2960");
  await expect(height).toHaveValue("480");

  // Later specs (triangular calibration) need the stock machine geometry back.
  await kit.selectOption(original.kit);
  await radius.fill(original.radius);
  await feeds.selectOption(original.feeds);
  await spacing.fill(original.spacing);
  await height.fill(original.height);
  await page.getByRole("button", { name: "Save configuration" }).click();
  await expect(page.getByText("Saved. The controller has the new machine dimensions.")).toBeVisible();
});

test("Z-axis and sprocket screens drive the machine with no controller attached", async ({ page }) => {
  await page.goto("/next/machine/z-axis");
  await expect(page.getByText(/^Server: connected/)).toBeVisible({ timeout: 15_000 });
  const step = page.getByRole("textbox", { name: "Step" });
  await expect(step).not.toHaveValue("");
  await step.fill("2");
  await page.getByRole("button", { name: "↑ Raise" }).click();
  await expect(page.getByRole("button", { name: "Touch off Z" })).toBeDisabled();

  await page.getByRole("link", { name: "← All machine setup" }).click();
  await page.getByRole("link", { name: "Sprockets and chains" }).click();
  const extendLeft = page.getByRole("button", { name: /^Extend left chain to \d+ mm$/ });
  await expect(extendLeft).toBeDisabled();
  await page.getByRole("button", { name: "Define zero" }).click();
  await expect(extendLeft).toBeEnabled();
  await page.getByRole("button", { name: "Left motor clockwise 10°" }).click();
  await expect(extendLeft).toBeDisabled();
});
