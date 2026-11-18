// An alarm mid-scan is in rust/crates/wc-server/tests/optical_e2e.rs: the UI cannot make the simulator alarm.
import { expect, test } from "@playwright/test";
import type { Locator, Page } from "@playwright/test";
import { recordLog, saveSettings, simulatorURL } from "./simulator";

// The simulator is built to the default geometry (motor spacing 2978.4, motor height 463); these settings are 3 mm
// narrower and 5 mm higher, so the synthetic camera sees each square off where the scan expects it.
const BUILT = { motorSpacingX: 2978.4, motorOffsetY: 463 };
const CONFIGURED = { motorSpacingX: "2975.4", motorOffsetY: "468" };
const SCREENS = process.env.E2E_SCREENS ?? "e2e/screens";
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, ipad: { width: 1024, height: 768 } };

test.use({ baseURL: simulatorURL, viewport: VIEWPORTS.desktop });
test.describe.configure({ mode: "serial" });
test.skip(({ isMobile }) => isMobile, "one simulator scan is enough; the iPad layout is shot from the desktop run");

const status = (page: Page) => page.getByRole("status", { name: "Calibration status" });
const scanCard = (page: Page) => page.locator(".card").filter({ has: page.getByRole("heading", { name: "Scan" }) });

/** What a viewer sees with `focus` scrolled into view; a full-page shot would misplace the sticky bar and dialogs. */
const shoot = async (page: Page, name: string, focus: Locator) => {
  await Object.entries(VIEWPORTS).reduce(async (previous, [label, size]) => {
    await previous;
    await page.setViewportSize(size);
    await focus.scrollIntoViewIfNeeded();
    await page.waitForTimeout(300);
    await page.screenshot({ path: `${SCREENS}/optical-${name}-${label}.png` });
  }, Promise.resolve());
  await page.setViewportSize(VIEWPORTS.desktop);
};

const openOptical = async (page: Page) => {
  await page.goto("/calibration/optical");
  await expect(page.getByRole("form", { name: "Optical calibration settings" })).toBeVisible();
};

/** Fills the scan extents and a quick motion: G1 at 3000 mm/min, 0.1 s to settle. */
const extents = async (page: Page, [tlX, tlY, brX, brY]: number[]) => {
  const form = page.getByRole("form", { name: "Optical calibration settings" });
  await form.getByText("Scan extents and motion").click();
  await form.getByLabel("Calibration extents").selectOption("Custom");
  const fields = { "X of top left": tlX, "Y of top left": tlY, "X of bottom right": brX, "Y of bottom right": brY };
  const motion = { "Feed rate between squares": 3000, "Settle time": 0.1 };
  await Object.entries({ ...fields, ...motion }).reduce(async (previous, [label, value]) => {
    await previous;
    await form.getByRole("textbox", { name: label }).fill(String(value));
  }, Promise.resolve());
};

const scanState = (page: Page, text: string | RegExp, timeout = 15_000) =>
  expect(scanCard(page).getByText(text).first()).toBeVisible({ timeout });

test.beforeAll(async ({ request }) => {
  await saveSettings(request, "camera", { cameraSource: "synthetic", fps: "30" });
  await saveSettings(request, "maslow", { zAxis: true, ...CONFIGURED });
});

test("with the setting off nothing optical shows or answers", async ({ page, request }) => {
  await saveSettings(request, "webcontrol", { opticalCalibrationEnabled: false });
  const state = await (await request.get("/api/state")).json();
  expect(state.experimental).toEqual([]);
  const refused = await request.get("/api/calibration/optical");
  expect(refused.status()).toBe(409);
  expect((await refused.json()).error).toContain("Optical calibration is turned off.");
  await page.goto("/calibration");
  await expect(page.getByRole("navigation", { name: "Calibration methods" }).getByRole("link")).toHaveText([
    "Triangular",
    "Holey",
  ]);
});

test("a scan finds the misconfigured geometry, the fit recovers it and Accept writes four settings", async ({
  page,
  request,
}) => {
  test.setTimeout(300_000);
  await saveSettings(request, "webcontrol", { opticalCalibrationEnabled: true });
  await openOptical(page);
  await expect(page.getByText("Source: synthetic")).toBeVisible();

  await page.getByRole("button", { name: "Test image" }).click();
  await expect(status(page)).toHaveText(/Found the square .* mm right and .* mm up of the centre/, { timeout: 20_000 });
  await expect(page.getByRole("img", { name: "Test image" })).toBeVisible();

  await extents(page, [-2, 2, 2, -2]);
  await page.getByRole("button", { name: "Start scan" }).click();
  await expect(status(page)).toHaveText("Scan started: 25 squares. Stop halts it.");
  await expect(page.getByText(/^Camera turned -?0\.\d°, not mirrored, 6\.\d\d px\/mm\.$/)).toBeVisible({
    timeout: 30_000,
  });
  await scanState(page, /^Measuring|^Moving|^Settling/);
  await shoot(page, "scanning", scanCard(page));
  await scanState(page, /^Done/, 240_000);
  const bar = page.getByRole("progressbar", { name: "Scan progress" });
  await expect(bar).toHaveAttribute("value", "25");
  await expect(status(page)).toHaveText("Scan done: 25 of 25 squares.");
  const spansCard = await bar.evaluate((progress) => {
    const card = progress.parentElement!;
    const { paddingLeft, paddingRight } = getComputedStyle(card);
    const inner = card.clientWidth - parseFloat(paddingLeft) - parseFloat(paddingRight);
    return Math.abs(progress.getBoundingClientRect().width - inner) < 1;
  });
  expect(spansCard, "the scan progress bar fills its card").toBe(true);
  await shoot(page, "scanned", scanCard(page));

  await page.getByRole("button", { name: "Fit geometry" }).click();
  await expect(status(page)).toHaveText(/Review the fitted values/, { timeout: 30_000 });
  await expect(page.getByText(/^From 25 squares, RMS residual 0\.0\d mm\./)).toBeVisible();
  const fitted = async (name: string) =>
    Number(
      await page
        .getByRole("row", { name: new RegExp(`^${name}`) })
        .getByRole("cell")
        .nth(1)
        .textContent(),
    );
  expect(Math.abs((await fitted("Distance between motors")) - BUILT.motorSpacingX)).toBeLessThan(0.5);
  expect(Math.abs((await fitted("Height of motors")) - BUILT.motorOffsetY)).toBeLessThan(0.5);

  await page.getByRole("button", { name: "Accept…" }).click();
  const confirm = page.getByRole("dialog", { name: "Write the fitted geometry?" });
  await expect(confirm.getByRole("listitem")).toHaveText([
    /^Distance between motors: 2975\.4 → 2978\.\d+$/,
    /^Height of motors: 468 → 46[23]\.\d+$/,
    /^Left chain tolerance: 0 → /,
    /^Right chain tolerance: 0 → /,
  ]);
  await shoot(page, "accept", confirm);
  const log = await recordLog(simulatorURL);
  await confirm.getByRole("button", { name: "Write settings" }).click();
  await expect(status(page)).toHaveText(/^Accepted\./);
  await page.waitForTimeout(1000);
  log.stop();
  const writes = log.lines
    .filter((line) => line.includes('"alog"'))
    .flatMap((line) => [...line.matchAll(/Sent: (\$\d+)=/g)].map((match) => match[1]));
  expect(writes).toEqual(["$3", "$2", "$40", "$41"]);
  const maslow = await (await request.get("/api/settings/maslow")).json();
  const saved = (key: string) => Number(maslow.settings.find((o: { key: string }) => o.key === key).value);
  expect(Math.abs(saved("motorSpacingX") - BUILT.motorSpacingX)).toBeLessThan(0.5);
  expect(Math.abs(saved("motorOffsetY") - BUILT.motorOffsetY)).toBeLessThan(0.5);
  await shoot(page, "accepted", page.getByRole("button", { name: "Fit geometry" }));
});

test("Stop mid-scan halts it and keeps the squares already measured", async ({ page, request }) => {
  await saveSettings(request, "webcontrol", { opticalCalibrationEnabled: true });
  await openOptical(page);
  await page.getByRole("button", { name: "Clear measurements" }).click();
  await extents(page, [-1, 1, 1, 0]);
  await page.getByRole("button", { name: "Start scan" }).click();
  await expect(status(page)).toHaveText("Scan started: 6 squares. Stop halts it.");
  await expect(page.getByRole("progressbar", { name: "Scan progress" })).toHaveAttribute("value", "2", {
    timeout: 60_000,
  });
  await scanCard(page).getByRole("button", { name: "Stop", exact: true }).click();
  await scanState(page, /^Stopped/);
  await expect(page.getByRole("button", { name: "Start scan" })).toBeEnabled();
  expect((await request.get("/api/calibration/optical/csv")).status()).toBe(200);
  await shoot(page, "stopped", scanCard(page));
});

test("turning the setting off hides the screen again and refuses the routes at once", async ({ page, request }) => {
  await saveSettings(request, "webcontrol", { opticalCalibrationEnabled: false });
  await page.goto("/calibration/optical");
  await expect(page.getByRole("alert")).toContainText("Optical calibration is turned off.");
  expect((await request.post("/api/calibration/optical/fit", { data: {} })).status()).toBe(409);
});
