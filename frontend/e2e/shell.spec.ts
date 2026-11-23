import { expect, test } from "@playwright/test";
import type { APIRequestContext, Page } from "@playwright/test";
import { chooseMenu, closeServerDialogs, connectBackend, openActions, resetMachine, waitForServer } from "./helpers";
import { simulatorURL } from "./simulator";

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
  const setTheme = async (request: APIRequestContext, theme: string) =>
    expect(await request.put("/api/display/theme", { data: { theme } })).toBeOK();
  const background = (page: Page) => page.evaluate(() => getComputedStyle(document.body).backgroundColor);
  const choose = async (page: Page, theme: string) => {
    const group = page.getByRole("dialog", { name: "Settings" }).getByRole("group", { name: "Display theme" });
    await group.getByRole("radio", { name: theme }).check();
  };

  test.afterEach(({ request }) => setTheme(request, "system"));

  test("applies a choice in Settings at once, and every open page follows without a reload", async ({
    page,
    context,
  }) => {
    await expect(page.getByRole("banner").getByRole("button", { name: /theme/i })).toHaveCount(0);
    const other = await context.newPage();
    await other.goto("/");
    await waitForServer(other);

    await chooseMenu(page, "Settings", "Display");
    await expect(page.getByRole("dialog", { name: "Settings" }).getByRole("button", { name: "Save" })).toHaveCount(0);
    await choose(page, "Light");
    await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
    await expect(other.locator("html")).toHaveAttribute("data-theme", "light");
    expect(await background(other)).toBe("rgb(255, 255, 255)");
    await choose(page, "Dark");
    await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
    await expect(other.locator("html")).toHaveAttribute("data-theme", "dark");
    expect(await background(other)).toBe("rgb(20, 23, 28)");

    await other.reload();
    await expect(other.locator("html")).toHaveAttribute("data-theme", "dark");
    await choose(page, "System");
    await expect(other.locator("html")).not.toHaveAttribute("data-theme");
  });

  test.describe("on the simulator", () => {
    test.use({ baseURL: simulatorURL });
    test.skip(({ isMobile }) => isMobile, "one simulated job is enough");
    test.afterEach(() => resetMachine(simulatorURL));

    test("changes mid-job without stopping it", async ({ page, request }) => {
      await request.post("/api/files/gcode/directories", { data: { name: "theme" } });
      const lines = ["G21", "G90", ...Array.from({ length: 400 }, (_, n) => `G1 X${(n % 20) / 10} Y0 F3000`)];
      const file = { name: "theme.nc", mimeType: "text/plain", buffer: Buffer.from(`${lines.join("\n")}\n`) };
      const upload = await request.post("/uploadGCode", { multipart: { selectedDirectory: "theme", "file[]": file } });
      expect(upload.status()).toBe(200);
      await page.goto("/");
      await waitForServer(page);
      await page.getByRole("button", { name: "Play" }).click();
      const line = async () => (await (await request.get("/api/main")).json()).gcode.lineIndex as number;
      await expect.poll(line, { timeout: 15_000 }).toBeGreaterThan(3);

      await chooseMenu(page, "Settings", "Display");
      await choose(page, "Dark");
      await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
      expect((await (await request.get("/api/state")).json()).theme).toBe("dark");
      expect((await (await request.get("/api/state")).json()).jobInProgress).toBe(true);
    });
  });

  test("starts each load in the theme last seen, then follows the server's", async ({ page, request }) => {
    await page.evaluate(() => localStorage.setItem("webcontrol.theme", "dark"));
    await page.reload();
    await expect(page.locator("html")).not.toHaveAttribute("data-theme");
    await expect.poll(() => page.evaluate(() => localStorage.getItem("webcontrol.theme"))).toBeNull();

    await setTheme(request, "light");
    await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
    await page.route("**/api/state", (route) => route.abort());
    await page.reload();
    await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  });
});
