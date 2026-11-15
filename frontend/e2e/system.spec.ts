import { expect, test } from "@playwright/test";
import { resetMachine, waitForServer } from "./helpers";

test.beforeEach(({ baseURL }) => resetMachine(baseURL!));

test("destructive actions ask first, and cancelling leaves the Actions modal usable", async ({ page }) => {
  await page.goto("/system");
  await expect(page).toHaveURL(/\/actions$/);
  await waitForServer(page);
  const actions = page.getByRole("dialog", { name: "Actions" });

  await actions.getByRole("button", { name: "Wipe controller EEPROM" }).click();
  const confirm = page.getByRole("dialog", { name: "Wipe controller EEPROM?" });
  await expect(confirm).toBeVisible();
  await confirm.getByRole("button", { name: "Cancel" }).click();
  await expect(confirm).toBeHidden();

  await actions.getByRole("button", { name: "Shut down WebControl" }).click();
  await expect(page.getByRole("dialog", { name: "Shut down WebControl?" })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog", { name: "Shut down WebControl?" })).toBeHidden();
  await expect(actions).toBeVisible();

  await actions.getByRole("button", { name: "Sync settings" }).click();
  await expect(actions.getByText("Sent “Sync settings”.")).toBeVisible();
});

test("PID tuning loads the saved gains and starts a velocity test", async ({ page }) => {
  await page.goto("/system/pid");
  await expect(page).toHaveURL(/\/pid$/);
  const velocity = page.getByRole("region", { name: "Velocity PID test" });
  await expect(velocity.getByRole("textbox", { name: "Kp" })).not.toHaveValue("");

  await velocity.getByRole("textbox", { name: "Start" }).fill("0");
  await velocity.getByRole("textbox", { name: "Stop" }).fill("10");
  await velocity.getByRole("textbox", { name: "Steps" }).fill("2");
  await velocity.getByRole("button", { name: "Run velocity test" }).click();
  await expect(velocity.getByText(/Test sent/)).toBeVisible();
});

test("offline help renders docs with working links and images", async ({ page }) => {
  await page.goto("/help");
  const article = page.locator("article");
  await expect(article.getByRole("heading", { name: "WebControl", level: 1 })).toBeVisible();
  await expect
    .poll(() => article.getByRole("img", { name: "Screenshot" }).evaluate((img: HTMLImageElement) => img.naturalWidth))
    .toBeGreaterThan(0);

  await page.getByRole("navigation", { name: "Help pages" }).getByRole("link", { name: "First Steps" }).click();
  await expect(page).toHaveURL(/\/help\/GettingStarted\/firstSteps\.md$/);
  await expect(article.getByRole("heading", { level: 1 })).toBeVisible();
});

test("the logs page connects to the log stream, without the app chrome", async ({ page }) => {
  await page.goto("/logs");
  await expect(page.getByText(/^Logging (active|suspended)$/)).toBeVisible({ timeout: 15_000 });
  await expect(page.getByRole("region", { name: "Abridged log" })).toBeVisible();
  await expect(page.getByRole("navigation", { name: "Primary" })).toHaveCount(0);
});

test("Help ▸ Logs opens the logs in a new tab", async ({ page, isMobile }) => {
  await page.goto("/");
  await waitForServer(page);
  if (isMobile) await page.getByRole("button", { name: "Menu", exact: true }).click();
  await page.getByRole("navigation", { name: "Primary" }).getByRole("button", { name: "Help" }).click();
  const [tab] = await Promise.all([
    page.context().waitForEvent("page"),
    page.getByRole("menuitem", { name: "Logs" }).click(),
  ]);
  await expect(tab).toHaveURL(/\/logs$/);
  await tab.close();
});
