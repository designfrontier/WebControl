import { expect, test } from "@playwright/test";
import type { APIRequestContext, Page } from "@playwright/test";
import { resetMachine, waitForServer } from "./helpers";
import { simulatorURL } from "./simulator";

const SCREENS = process.env.E2E_SCREENS ?? "e2e/screens";
const FOLDER = "pacing";

const setPacing = async (request: APIRequestContext, mode: string, gap: number) =>
  expect(
    await request.put("/api/run/pacing", { data: { values: { linePacingMode: mode, linePacingGapMs: gap } } }),
  ).toBeOK();

const pacing = (page: Page) => page.getByRole("group", { name: "Line pacing" });

test("a preset saves the pacing settings", async ({ page, baseURL, request }) => {
  await resetMachine(baseURL!);
  await page.goto("/");
  await waitForServer(page);
  await pacing(page).getByLabel("Pacing").selectOption({ label: "Min gap · 25 ms" });
  await expect
    .poll(async () => {
      const section = await (await request.get("/api/settings/webcontrol")).json();
      const value = (key: string) => section.settings.find((field: { key: string }) => field.key === key).value;
      return [value("linePacingMode"), value("linePacingGapMs")];
    })
    .toEqual(["minimum gap", 25]);
  await setPacing(request, "v0.94", 50);
});

test.describe("against the simulator", () => {
  test.use({ baseURL: simulatorURL, viewport: { width: 1280, height: 800 } });
  test.skip(({ isMobile }) => isMobile, "one simulated job is enough");

  test("a job shows its rate, and a change mid-job ends a segment in the history", async ({ page, request }) => {
    await setPacing(request, "v0.94", 50);
    await request.post("/api/files/gcode/directories", { data: { name: FOLDER } });
    const lines = ["G21", "G90", ...Array.from({ length: 400 }, (_, n) => `G1 X${(n % 20) / 10} Y0 F3000`)];
    const file = { name: "pacing.nc", mimeType: "text/plain", buffer: Buffer.from(`${lines.join("\n")}\n`) };
    const upload = await request.post("/uploadGCode", { multipart: { selectedDirectory: FOLDER, "file[]": file } });
    expect(upload.status()).toBe(200);

    await page.goto("/");
    await waitForServer(page);
    await page.getByRole("button", { name: "Play" }).click();
    await expect(pacing(page).getByText(/^\d+ lines\/min$/)).toBeVisible({ timeout: 15_000 });
    await expect(pacing(page)).toContainText(/Sent \d+ · Lost 0\.00%/);
    await pacing(page).getByLabel("Pacing").selectOption({ label: "Min gap · 25 ms" });
    await expect(pacing(page).getByText(/^\d+ lines\/min$/)).toBeVisible({ timeout: 5_000 });
    await page.getByRole("region", { name: "Run" }).screenshot({ path: `${SCREENS}/pacing-run-panel-desktop.png` });
    await page.setViewportSize({ width: 1024, height: 768 });
    await page.waitForTimeout(300);
    await page.getByRole("region", { name: "Run" }).screenshot({ path: `${SCREENS}/pacing-run-panel-ipad.png` });

    await pacing(page)
      .getByRole("button", { name: /lines\/min/ })
      .click();
    const details = page.getByRole("dialog", { name: "Line pacing" });
    await expect(details).toContainText("Pacing now: min gap 25 ms");
    await expect(details.getByRole("row").nth(1)).toContainText("pacing change");
    await details.screenshot({ path: `${SCREENS}/pacing-details-ipad.png` });
    await details.getByRole("button", { name: "Close" }).click();
    await page.getByRole("button", { name: "Stop" }).first().click();
    await setPacing(request, "v0.94", 50);
  });
});
