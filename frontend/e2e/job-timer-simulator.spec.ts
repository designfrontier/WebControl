import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";
import { waitForServer } from "./helpers";
import { saveSettings, simulatorURL } from "./simulator";

/** 300 moves of 20 mm at 600 mm/min: an estimate of 10 minutes, which the simulator runs far faster. */
const ZIGZAG = Array.from({ length: 300 }, (_, at) => `G1 X${at % 2 === 0 ? 20 : 0} F600`).join("\n");
const SHOTS = process.env.TIMER_SHOTS ?? "e2e/screens";
const SIZES = {
  desktop: { width: 1280, height: 800 },
  ipad: { width: 1024, height: 768 },
  phone: { width: 390, height: 844 },
};

test.use({ baseURL: simulatorURL, viewport: SIZES.desktop });
test.skip(({ isMobile }) => isMobile, "one simulated run is enough; the phone layout is shot from the desktop run");

const shoot = async (page: Page, name: string) => {
  await Object.entries(SIZES).reduce(async (previous, [label, size]) => {
    await previous;
    await page.setViewportSize(size);
    await ["light", "dark"].reduce(async (before, scheme) => {
      await before;
      await page.emulateMedia({ colorScheme: scheme as "light" | "dark" });
      await page.getByLabel("Machine readouts").getByText("Job", { exact: true }).scrollIntoViewIfNeeded();
      await page.waitForTimeout(150);
      await page.screenshot({ path: `${SHOTS}/timer-${name}-${label}-${scheme}.png` });
    }, Promise.resolve());
  }, Promise.resolve());
  await page.setViewportSize(SIZES.desktop);
  await page.emulateMedia({ colorScheme: "light" });
};

test("a job shows the file's estimate, then its wall, cutting and remaining time", async ({ page, request }) => {
  test.setTimeout(300_000);
  await saveSettings(request, "maslow", { zAxis: true });
  await page.goto("/");
  await waitForServer(page);
  const file = { name: "timer.nc", mimeType: "text/plain", buffer: Buffer.from(`G21\nG90\n${ZIGZAG}\n`) };
  expect(await request.post("/uploadGCode", { multipart: { selectedDirectory: ".", "file[]": file } })).toBeOK();
  const run = page.getByRole("region", { name: "Run" });
  await expect(run.getByText("File:").locator("..")).toContainText("timer.nc · est. 10m");
  await shoot(page, "loaded");

  await page.locator(".run-buttons").getByRole("button", { name: "Play" }).click();
  const time = page.getByLabel("Machine readouts").getByText("Job", { exact: true }).locator("+ dd");
  await expect(time).toHaveText(/\d+s · cutting \d+s · ~\d+[sm].* left$/, { timeout: 20_000 });
  await shoot(page, "running");
  await page.locator(".run-buttons").getByRole("button", { name: "Stop", exact: true }).click();
  await expect(time).toHaveText(/\d+s · cutting \d+s$/, { timeout: 20_000 });
});
