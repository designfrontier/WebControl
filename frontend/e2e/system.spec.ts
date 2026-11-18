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

test("the Update screen says when GitHub can't be reached and how to update on the Pi", async ({ page }) => {
  await page.goto("/updates");
  const updates = page.getByRole("dialog", { name: "Update" });
  await expect(updates.getByRole("heading", { name: "Couldn't reach GitHub" })).toBeVisible();
  await expect(updates.getByRole("alert")).toContainText("Couldn't reach GitHub");
  await expect(updates.getByText(/^webcontrol-rs \d+\.\d+\.\d+.* \(commit \w+\)$/)).toBeVisible();
  const steps = updates.getByRole("region", { name: "How to update" });
  await expect(steps.getByText("webcontrol-update", { exact: true })).toBeVisible();
  await expect(steps.getByText("ssh pi@localhost")).toBeVisible();
});

const release = {
  installed: { version: "0.3.0", commit: "dc9bdca" },
  repo: "designfrontier/WebControl",
  latest: {
    tag: "v0.4.0",
    name: "webcontrol-rs 0.4.0",
    publishedAt: "2026-11-20T10:00:00Z",
    url: "https://github.com/designfrontier/WebControl/releases/tag/v0.4.0",
    notes: "## Faster previews\n\n<script>window.injected = true</script>",
    commit: null,
  },
  status: "updateAvailable",
  error: null,
  checkedAt: Date.now(),
  backup: { version: "0.2.0", date: "2026-11-16T18:00:00Z" },
  canUpdate: true,
  busy: null,
  progress: null,
};

const run = (phase: string, over: object = {}) => ({
  action: "update",
  phase,
  running: true,
  from: "0.3.0",
  to: "0.4.0",
  startedAt: Date.now(),
  finishedAt: null,
  result: null,
  log: ["Updating webcontrol-rs 0.3.0 to 0.4.0 from designfrontier/WebControl."],
  ...over,
});

test("the Update screen shows a newer release with its notes, and updates through a restart", async ({ page }) => {
  // The server's answers as an update goes: started, installing, gone while it restarts, then done.
  const answers: (object | null)[] = [run("installing"), null, null, run("healthCheck")];
  const done = run("finished", { running: false, result: "updated", finishedAt: Date.now() });
  let started = false;
  await page.route("**/api/system/updates", (route) => {
    if (!started) return route.fulfill({ json: release });
    const next = answers.length > 0 ? answers.shift() : done;
    return next ? route.fulfill({ json: { ...release, progress: next } }) : route.abort("connectionrefused");
  });
  await page.route("**/api/system/updates/install", (route) => {
    started = true;
    return route.fulfill({ status: 202, json: { ...release, progress: run("starting", { log: [] }) } });
  });
  await page.goto("/updates");
  const updates = page.getByRole("dialog", { name: "Update" });
  await expect(updates.getByRole("heading", { name: "Update available: v0.4.0" })).toBeVisible();
  await expect(updates.getByRole("heading", { name: "Faster previews" })).toBeVisible();
  await expect(updates.getByText(/^0\.2\.0 from /)).toBeVisible();
  await expect(updates.getByRole("link", { name: "View on GitHub" })).toHaveAttribute("target", "_blank");

  const button = updates.getByRole("button", { name: "Update to v0.4.0" });
  expect((await button.boundingBox())!.height).toBeGreaterThanOrEqual(44);
  await button.click();
  const confirm = page.getByRole("dialog", { name: "Update to v0.4.0?" });
  await expect(confirm.getByRole("heading", { name: "Faster previews" })).toBeVisible();
  await confirm.getByRole("button", { name: "Update now" }).click();

  const progress = updates.getByRole("region", { name: "Update progress" });
  await expect(progress.getByRole("heading", { name: "Updating to v0.4.0" })).toBeVisible();
  await expect(progress.getByText("WebControl is restarting; reconnecting…")).toBeVisible();
  const last = updates.getByRole("region", { name: "Last update" });
  await expect(last.getByText("Updated from 0.3.0 to 0.4.0.")).toBeVisible({ timeout: 20_000 });
  await expect(last.getByRole("button", { name: "Reload the page" })).toBeVisible();
  expect(await page.evaluate(() => "injected" in window)).toBe(false);
});

test("this server has no updater units, so the Update screen offers no buttons", async ({ page }) => {
  await page.goto("/updates");
  const updates = page.getByRole("dialog", { name: "Update" });
  await expect(updates.getByRole("heading", { name: "How to update" })).toBeVisible();
  await expect(updates.getByRole("button", { name: /Update to|Roll back/ })).toHaveCount(0);
});
