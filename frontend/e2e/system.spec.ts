import { expect, test } from "@playwright/test";

test("destructive actions ask first, and cancelling leaves the page usable", async ({ page }) => {
  await page.goto("/next/system");
  await expect(page.getByText(/^Server: connected/)).toBeVisible({ timeout: 15_000 });

  await page.getByRole("button", { name: "Wipe controller EEPROM" }).click();
  const dialog = page.getByRole("dialog", { name: "Wipe controller EEPROM?" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Cancel" }).click();
  await expect(dialog).toBeHidden();

  await page.getByRole("button", { name: "Shut down WebControl" }).click();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toBeHidden();

  await page.getByRole("button", { name: "Sync settings" }).click();
  await expect(page.getByText("Sent “Sync settings”.")).toBeVisible();
});

test("PID tuning loads the saved gains and starts a velocity test", async ({ page }) => {
  await page.goto("/next/system/pid");
  const velocity = page.getByRole("region", { name: "Velocity PID test" });
  await expect(velocity.getByRole("textbox", { name: "Kp" })).not.toHaveValue("");

  await velocity.getByRole("textbox", { name: "Start" }).fill("0");
  await velocity.getByRole("textbox", { name: "Stop" }).fill("10");
  await velocity.getByRole("textbox", { name: "Steps" }).fill("2");
  await velocity.getByRole("button", { name: "Run velocity test" }).click();
  await expect(velocity.getByText(/Test sent/)).toBeVisible();
});

test("offline help renders docs with working links and images", async ({ page }) => {
  await page.goto("/next/system/help");
  const article = page.locator("article");
  await expect(article.getByRole("heading", { name: "WebControl", level: 1 })).toBeVisible();
  await expect
    .poll(() => article.getByRole("img", { name: "Screenshot" }).evaluate((img: HTMLImageElement) => img.naturalWidth))
    .toBeGreaterThan(0);

  await page.getByRole("navigation", { name: "Help pages" }).getByRole("link", { name: "First Steps" }).click();
  await expect(page).toHaveURL(/\/next\/system\/help\/GettingStarted\/firstSteps\.md$/);
  await expect(article.getByRole("heading", { level: 1 })).toBeVisible();
});

test("the logs page connects to the log stream", async ({ page }) => {
  await page.goto("/next/system/logs");
  await expect(page.getByText(/^Logging (active|suspended)$/)).toBeVisible({ timeout: 15_000 });
  await expect(page.getByRole("region", { name: "Abridged log" })).toBeVisible();
});
