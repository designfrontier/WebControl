import { expect, test } from "@playwright/test";
import type { APIRequestContext, Page } from "@playwright/test";
import { chooseMenu, closeServerDialogs, connectBackend, openActions, resetMachine, waitForServer } from "./helpers";

test.beforeEach(async ({ page, baseURL }) => {
  await resetMachine(baseURL!);
  await page.goto("/");
  await waitForServer(page);
  await closeServerDialogs(page);
});

test("the menus open the right modals", async ({ page }) => {
  const cases: [string, string, RegExp, string][] = [
    ["G-Code", "Open", /\/files\/open$/, "Open G-Code"],
    ["G-Code", "Send Custom GCode", /\/files\/send$/, "Send Custom G-Code"],
    ["Settings", "Advanced", /\/settings\/advanced$/, "Settings"],
    ["Boards", "Trim Board", /\/boards\/trim$/, "Trim Board"],
    ["Help", "About", /\/about$/, "About"],
  ];
  await cases.reduce(async (previous, [menu, item, url, title]) => {
    await previous;
    await chooseMenu(page, menu, item);
    await expect(page).toHaveURL(url);
    const dialog = page.getByRole("dialog", { name: title });
    await expect(dialog).toBeVisible();
    await dialog.getByRole("button", { name: "Close", exact: true }).click();
    await expect(dialog).toBeHidden();
  }, Promise.resolve());

  await openActions(page);
  await expect(page.getByRole("dialog", { name: "Actions" })).toBeVisible();
});

test("Back closes a modal and a reload restores it", async ({ page }) => {
  await chooseMenu(page, "Settings", "GPIO");
  const settings = page.getByRole("dialog", { name: "Settings" });
  await expect(settings.getByRole("form", { name: "GPIO Settings" })).toBeVisible();

  await page.reload();
  await expect(settings.getByRole("form", { name: "GPIO Settings" })).toBeVisible();

  await settings.getByRole("navigation", { name: "Settings sections" }).getByRole("link", { name: "Camera" }).click();
  await expect(page).toHaveURL(/\/settings\/camera$/);
  await page.goBack();
  await expect(page).toHaveURL(/\/$/);
  await expect(settings).toBeHidden();
});

test("a server alert stacks above an open modal and leaves it usable", async ({ page, baseURL }) => {
  await openActions(page);
  const actions = page.getByRole("dialog", { name: "Actions" });
  await expect(actions).toBeVisible();

  const socket = await connectBackend(baseURL!);
  try {
    const play = () => socket.emit("action", { data: { command: "startRun", arg: "", arg1: "" } });
    play();
    play();
    const alert = page.getByRole("alertdialog");
    await expect(alert).toBeVisible();
    await expect(page.locator("dialog[open][data-kind]")).toHaveCount(1);

    // The alert is on top: Actions behind it is inert, so its buttons cannot be clicked.
    const sync = actions.getByRole("button", { name: "Sync settings" });
    await expect(sync.click({ trial: true, timeout: 1_000 })).rejects.toThrow();
    await alert.getByRole("button", { name: "Stop machine" }).click({ trial: true, timeout: 1_000 });

    const seen = await closeServerDialogs(page);
    expect(seen.join("\n")).toContain("No GCode file loaded.");
    await expect(page.locator("dialog[open]")).toHaveCount(1);
    await actions.getByRole("button", { name: "Sync settings" }).click();
    await expect(actions.getByText("Sent “Sync settings”.")).toBeVisible();
  } finally {
    socket.disconnect();
  }
});

test("the control screen fits 1440x900 and 1280x800 without page scroll", async ({ page, isMobile }) => {
  test.skip(isMobile, "Phones stack the viewer above the controls and scroll.");
  await [
    { width: 1440, height: 900 },
    { width: 1280, height: 800 },
  ].reduce(async (previous, size) => {
    await previous;
    await page.setViewportSize(size);
    await page.reload();
    await waitForServer(page);
    const overflow = await page.evaluate(() => [
      document.documentElement.scrollHeight - innerHeight,
      document.documentElement.scrollWidth - innerWidth,
    ]);
    expect(overflow).toEqual([0, 0]);
    await expect(page.getByRole("button", { name: "Stop", exact: true })).toBeInViewport();
    await expect(page.getByRole("log", { name: "Controller Messages" })).toBeInViewport();
  }, Promise.resolve());
});

test("Stop stays on screen on a phone", async ({ page, isMobile }) => {
  test.skip(!isMobile, "Phone layout only.");
  await page.mouse.wheel(0, 2000);
  await expect(page.getByRole("button", { name: "Stop", exact: true })).toBeInViewport();
  await page.mouse.wheel(0, -4000);
  await expect(page.getByRole("button", { name: "Stop", exact: true })).toBeInViewport();
});

test.describe("the display theme", () => {
  const setTheme = async (request: APIRequestContext, uiTheme: string) =>
    expect(await request.put("/api/settings/webcontrol", { data: { values: { uiTheme } } })).toBeOK();
  const background = (page: Page) => page.evaluate(() => getComputedStyle(document.body).backgroundColor);

  test.afterEach(({ request }) => setTheme(request, "system"));

  test("is a WebControl setting that every open page follows without a reload", async ({ page, context }) => {
    await expect(page.getByRole("banner").getByRole("button", { name: /theme/i })).toHaveCount(0);
    const other = await context.newPage();
    await other.goto("/");
    await waitForServer(other);

    const save = async (theme: string) => {
      await page.goto("/settings/webcontrol");
      const form = page.getByRole("form", { name: "WebControl Settings" });
      await form.getByLabel("Display theme").selectOption(theme);
      await form.getByRole("button", { name: "Save" }).click();
      await expect(form.getByText("WebControl Settings saved.")).toBeVisible();
    };

    await save("light");
    await expect(other.locator("html")).toHaveAttribute("data-theme", "light");
    expect(await background(other)).toBe("rgb(255, 255, 255)");
    await save("dark");
    await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
    await expect(other.locator("html")).toHaveAttribute("data-theme", "dark");
    expect(await background(other)).toBe("rgb(20, 23, 28)");

    await other.reload();
    await expect(other.locator("html")).toHaveAttribute("data-theme", "dark");
    await save("system");
    await expect(other.locator("html")).not.toHaveAttribute("data-theme");
  });

  test("replaces an older build's per-browser choice once the server's theme loads", async ({ page }) => {
    await page.evaluate(() => localStorage.setItem("webcontrol.theme", "dark"));
    await page.reload();
    await expect(page.locator("html")).not.toHaveAttribute("data-theme");
    await expect.poll(() => page.evaluate(() => localStorage.getItem("webcontrol.theme"))).toBeNull();
  });
});
