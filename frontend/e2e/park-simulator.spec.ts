import { expect, test } from "@playwright/test";
import type { APIRequestContext, Locator, Page } from "@playwright/test";
import { chooseMenu, closeServerDialogs, openActions, waitForServer } from "./helpers";
import { recordLog, saveSettings, simulatorURL } from "./simulator";

const SHOTS = process.env.PARK_SHOTS ?? "e2e/screens";
const SIZES = {
  desktop: { width: 1280, height: 800 },
  ipad: { width: 1024, height: 768 },
  phone: { width: 390, height: 844 },
};
const DEFAULT_BUTTONS = ["park", "zAxis", "setHome", "setPark"];

test.use({ baseURL: simulatorURL, viewport: SIZES.desktop });
test.describe.configure({ mode: "serial" });
test.skip(({ isMobile }) => isMobile, "one simulated run is enough; the phone layout is shot from the desktop run");

const setButtons = async (request: APIRequestContext, buttons: string[]) =>
  expect(await request.put("/api/display/controls", { data: { buttons } })).toBeOK();

test.beforeAll(async ({ request }) => {
  await saveSettings(request, "maslow", {
    zAxis: true,
    parkX: "-200",
    parkY: "100",
    macro3: "M5",
    macro3_title: "Vac",
  });
  await setButtons(request, DEFAULT_BUTTONS);
});

test.afterAll(async ({ request }) => {
  await saveSettings(request, "maslow", { parkX: "", parkY: "", macro3: "", macro3_title: "Macro 3" });
  await setButtons(request, DEFAULT_BUTTONS);
});

/** Every size, light and dark, of `part` (or the page), with `seen` scrolled into view. */
const shoot = async (page: Page, name: string, part?: Locator, seen: Locator | undefined = part) => {
  await Object.entries(SIZES).reduce(async (previous, [label, size]) => {
    await previous;
    await page.setViewportSize(size);
    await ["light", "dark"].reduce(async (before, scheme) => {
      await before;
      await page.emulateMedia({ colorScheme: scheme as "light" | "dark" });
      await seen?.scrollIntoViewIfNeeded();
      await page.waitForTimeout(150);
      const path = `${SHOTS}/park-${name}-${label}-${scheme}.png`;
      await (part ? part.screenshot({ path }) : page.screenshot({ path }));
    }, Promise.resolve());
  }, Promise.resolve());
  await page.setViewportSize(SIZES.desktop);
  await page.emulateMedia({ colorScheme: "light" });
};

const jog = (page: Page) => page.getByRole("region", { name: "Jog" });
const sled = (page: Page) =>
  page
    .getByRole("definition")
    .filter({ hasText: /^X -?\d/ })
    .first();
const logged = (lines: string[], text: string) => lines.some((line) => line.includes(text));

test("a short file that finishes parks the sled at the park position", async ({ page, request }) => {
  test.setTimeout(90_000);
  const text = "G21\nG90\nG0 Z5\nG0 X10 Y10\nG1 X20 F600\nG0 Z5\n";
  const file = { name: "park-short.nc", mimeType: "text/plain", buffer: Buffer.from(text) };
  expect(await request.post("/uploadGCode", { multipart: { selectedDirectory: ".", "file[]": file } })).toBeOK();
  await page.goto("/");
  await waitForServer(page);
  const log = await recordLog(simulatorURL);
  await page.getByRole("button", { name: "Play" }).click();
  await expect.poll(() => logged(log.lines, "PARK START trigger=file"), { timeout: 30_000 }).toBe(true);
  await expect.poll(() => logged(log.lines, "Sent: G00 X-200.0 Y100.0"), { timeout: 10_000 }).toBe(true);
  await expect(sled(page)).toHaveText(/^X -200\.00 Y 100\.00 Z 5\.00$/, { timeout: 15_000 });
  log.stop();
});

test("Park sends the sled there on demand, and is disabled with a reason while unset", async ({ page, request }) => {
  await page.goto("/");
  await waitForServer(page);
  await closeServerDialogs(page);
  await jog(page).getByRole("button", { name: "Right", exact: true }).click();
  await expect(sled(page)).not.toHaveText(/^X -200\.00 Y 100\.00/, { timeout: 10_000 });
  const park = jog(page).getByRole("button", { name: "Park", exact: true });
  await expect(park).toHaveAccessibleDescription("Parks at X -200, Y 100 mm (machine coordinates).");
  await park.click();
  await expect(sled(page)).toHaveText(/^X -200\.00 Y 100\.00/, { timeout: 15_000 });
  await shoot(page, "control-grid", jog(page));

  await saveSettings(request, "maslow", { parkX: "", parkY: "" });
  await expect(park).toBeDisabled();
  await expect(park).toHaveAccessibleDescription(/No park position yet/);
  await saveSettings(request, "maslow", { parkX: "-200", parkY: "100" });
  await expect(park).toBeEnabled();
});

test("a slot changed in Settings changes the grid on the control screen", async ({ page }) => {
  await page.goto("/");
  await waitForServer(page);
  await chooseMenu(page, "Settings", "Display");
  const section = page.getByRole("group", { name: "Control buttons" });
  await section.getByRole("combobox", { name: "Right column, top" }).selectOption({ label: "Macro 3: Vac" });
  await expect(section.getByRole("combobox", { name: "Right column, top" })).toHaveValue("macro3");
  await shoot(page, "settings-control-buttons", section);
  await page
    .getByRole("dialog", { name: "Settings" })
    .getByRole("button", { name: "Close", exact: true })
    .last()
    .click();
  await expect(jog(page).getByRole("button", { name: "Vac" })).toBeEnabled();
  await expect(jog(page).getByRole("button", { name: "Park", exact: true })).toHaveCount(0);
  await shoot(page, "control-grid-macro", jog(page));
});

test("Actions lists the four macros, and Settings holds the park position", async ({ page }) => {
  await page.goto("/");
  await waitForServer(page);
  await openActions(page);
  const macros = page.getByRole("region", { name: "Macros" });
  await expect(macros.getByRole("button")).toHaveText(["Macro 1", "Macro 2", "Vac", "Macro 4"]);
  await expect(macros.getByRole("button", { name: "Macro 1" })).toBeDisabled();
  await shoot(page, "actions-macros", macros);
  await page.goto("/settings");
  const parkX = page.getByLabel("Park Position X in MM");
  await expect(parkX).toHaveValue("-200");
  await shoot(page, "settings-park", undefined, parkX);
});
