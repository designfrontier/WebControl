import { expect, test } from "@playwright/test";

test("edits and saves Maslow settings, which survive a reload", async ({ page }, testInfo) => {
  const title = `M1 ${testInfo.project.name}`;
  await page.goto("/next/settings");
  const form = page.getByRole("form", { name: "Maslow Settings" });
  await expect(form).toBeVisible();

  await form.getByLabel("Macro 1 Title").fill(title);
  await form.getByLabel("Z-axis installed").check();
  await form.getByRole("button", { name: "Save" }).click();
  await expect(form.getByText("Maslow Settings saved.")).toBeVisible();
  await expect(form.getByRole("button", { name: "Save" })).toBeDisabled();

  await page.reload();
  await expect(form.getByLabel("Macro 1 Title")).toHaveValue(title);
  await expect(form.getByLabel("Z-axis installed")).toBeChecked();
});

test("rejects a non-numeric machine dimension without saving", async ({ page }) => {
  await page.goto("/next/settings");
  const form = page.getByRole("form", { name: "Maslow Settings" });
  const spacing = form.getByLabel("Distance Between Motors");
  const saved = await spacing.inputValue();

  await spacing.fill("wide");
  await form.getByRole("button", { name: "Save" }).click();
  await expect(form.getByText("Enter a number")).toBeVisible();

  await page.reload();
  await expect(spacing).toHaveValue(saved);
});

test("refreshes serial ports over the socket", async ({ page }) => {
  await page.goto("/next/settings");
  await expect(page.getByText(/^Server: connected/)).toBeVisible({ timeout: 15_000 });
  const ports = page.getByRole("form", { name: "Maslow Settings" }).getByLabel("Serial Connection");
  await expect(ports.getByRole("option", { name: "Not set" })).toBeAttached();

  await page.getByRole("button", { name: "Refresh ports" }).click();
  await expect(async () => expect(await ports.getByRole("option").count()).toBeGreaterThan(1)).toPass();
});

test("assigns a GPIO pin from the GPIO tab", async ({ page, isMobile }) => {
  const action = isMobile ? "Pause" : "Stop";
  await page.goto("/next/settings");
  await page.getByRole("navigation", { name: "Settings sections" }).getByRole("link", { name: "GPIO" }).click();
  await expect(page).toHaveURL(/\/next\/settings\/gpio$/);

  const form = page.getByRole("form", { name: "GPIO Settings" });
  await expect(page.getByRole("img", { name: "Raspberry Pi GPIO pin layout" })).toBeVisible();
  await form.getByLabel("GPIO4", { exact: true }).selectOption(action);
  await form.getByRole("button", { name: "Save" }).click();
  await expect(form.getByText("GPIO Settings saved.")).toBeVisible();

  await page.reload();
  await expect(form.getByLabel("GPIO4", { exact: true })).toHaveValue(action);
});
