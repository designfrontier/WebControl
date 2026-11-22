import { expect, test } from "@playwright/test";
import type { APIRequestContext, Page } from "@playwright/test";
import { chooseMenu, closeServerDialogs, waitForServer } from "./helpers";
import { saveSettings, simulatorURL } from "./simulator";

// Three files on the simulated controller: two with tool 1, run back to back, then one with tool 2.
const FILES = {
  "queue-a.nc": "T1 M6\nG21\nG90\nG0 Z5\nG1 X60 F150\nG1 X0\nG0 Z5\n",
  "queue-b.nc": "T1 M6\nG21\nG90\nG1 X5 F600\nG0 Z5\n",
  "queue-c.nc": "T2 M6\nG21\nG90\nG1 X10 F600\nG0 Z5\nM30\n",
};
const SHOTS = process.env.QUEUE_SHOTS ?? "e2e/screens";
const SIZES = {
  desktop: { width: 1280, height: 800 },
  ipad: { width: 1024, height: 768 },
  phone: { width: 390, height: 844 },
};

test.use({ baseURL: simulatorURL, viewport: SIZES.desktop });
test.describe.configure({ mode: "serial" });
test.skip(({ isMobile }) => isMobile, "one simulated run is enough; the phone layout is shot from the desktop run");

const upload = async (request: APIRequestContext, name: string, text: string) => {
  const file = { name, mimeType: "text/plain", buffer: Buffer.from(text) };
  expect(await request.post("/uploadGCode", { multipart: { selectedDirectory: ".", "file[]": file } })).toBeOK();
};

test.beforeAll(async ({ request }) => {
  await saveSettings(request, "maslow", { zAxis: true });
});

const queueState = async (request: APIRequestContext) => (await request.get("/api/queue")).json();

/** Every size, light and dark, of what the page shows now. */
const shoot = async (page: Page, name: string) => {
  await Object.entries(SIZES).reduce(async (previous, [label, size]) => {
    await previous;
    await page.setViewportSize(size);
    await ["light", "dark"].reduce(async (before, scheme) => {
      await before;
      await page.emulateMedia({ colorScheme: scheme as "light" | "dark" });
      await page.waitForTimeout(200);
      await page.screenshot({ path: `${SHOTS}/queue-${name}-${label}-${scheme}.png` });
    }, Promise.resolve());
  }, Promise.resolve());
  await page.setViewportSize(SIZES.desktop);
  await page.emulateMedia({ colorScheme: "light" });
};

const resumeAtPrompt = async (page: Page, tool: number) => {
  const prompt = page.locator("dialog[open][data-kind]").filter({ hasText: `Please insert tool ${tool}` });
  await expect(prompt).toBeVisible({ timeout: 20_000 });
  return prompt;
};

test("queued files run back to back, pausing only for the new tool", async ({ page, request }) => {
  test.setTimeout(180_000);
  await Promise.all(Object.entries(FILES).map(([name, text]) => upload(request, name, text)));
  await page.goto("/files/open");
  await waitForServer(page);
  await page.getByLabel("Directory").selectOption({ label: "Top level" });
  await Object.keys(FILES).reduce(async (previous, name) => {
    await previous;
    await page.getByRole("radio", { name }).click();
    await page.getByRole("button", { name: "Add to queue" }).click();
    await expect(page.getByText(`Added ${name} to the queue.`)).toBeVisible();
  }, Promise.resolve());

  await page.goto("/");
  await chooseMenu(page, "G-Code", "Job Queue");
  const list = page.getByRole("list", { name: "Queued files" });
  await expect(list.getByRole("listitem")).toHaveCount(3);
  await expect(list.getByRole("listitem").nth(2)).toContainText("Tool 2, will pause for a tool change");
  await shoot(page, "idle");

  // An earlier spec may have left tool 1 in the simulated spindle; then the first file needs no prompt.
  const firstNeedsTool = (await queueState(request)).items[0].between === "toolChange";
  await page.getByRole("button", { name: "Start queue" }).click();
  if (firstNeedsTool) await (await resumeAtPrompt(page, 1)).getByRole("button", { name: "Resume" }).click();
  await expect(list.getByRole("listitem").nth(1)).toContainText("same tool, starts immediately");
  await shoot(page, "started");
  await closeServerDialogs(page);
  await page
    .getByRole("dialog", { name: "Job Queue" })
    .getByRole("button", { name: "Close", exact: true })
    .last()
    .click();
  await shoot(page, "control-between-files");

  await expect.poll(async () => (await queueState(request)).step, { timeout: 30_000 }).toBe("toolChange");
  const between = await queueState(request);
  expect(between.items.map((item: { file: string }) => item.file)).toEqual(["queue-c.nc"]);
  expect(between.step).toBe("toolChange");
  await closeServerDialogs(page);
  await page.locator(".run-buttons").getByRole("button", { name: "Stop", exact: true }).click();
  const summary = page.getByRole("region", { name: "Job queue" });
  await expect(summary.getByRole("status")).toHaveText("Queue stopped: Stop was pressed.");
  await shoot(page, "control-stopped");

  // Stop at the prompt leaves tool 2 as the firmware's tool, so the third file now starts once Z has risen.
  await summary.getByRole("button", { name: "Continue queue" }).click();
  await expect.poll(async () => (await queueState(request)).items.length, { timeout: 30_000 }).toBe(0);
  expect((await queueState(request)).state).toBe("idle");
  await expect(summary).toBeHidden();
});
