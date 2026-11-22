import { expect, test } from "@playwright/test";
import type { APIRequestContext, Locator, Page } from "@playwright/test";
import { chooseMenu, closeServerDialogs, waitForServer } from "./helpers";
import { saveSettings, simulatorURL } from "./simulator";

/** Enough lines that the simulator, which paces each line but moves at once, is still cutting during the shots. */
const zigzag = Array.from({ length: 300 }, (_, at) => `G1 X${at % 2 === 0 ? 20 : 0} F600`).join("\n");

// Tool 1 twice, run back to back; then tool 2, which waits at the prompt; then tool 3 with its override on.
const FILES = {
  "queue-a.nc": `T1 M6\nG21\nG90\nG0 Z5\n${zigzag}\nG0 Z5\n`,
  "queue-b.nc": "T1 M6\nG21\nG90\nG1 X5 F600\nG0 Z5\n",
  "queue-c.nc": "T2 M6\nG21\nG90\nG1 X10 F600\nG0 Z5\n",
  "queue-d.nc": "T3 M6\nG21\nG90\nG1 X15 F600\nG0 Z5\nM30\n",
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

/** Every size, light and dark, of what the page shows now; `seen` is scrolled into view at each size. */
const shoot = async (page: Page, name: string, seen?: Locator) => {
  await Object.entries(SIZES).reduce(async (previous, [label, size]) => {
    await previous;
    await page.setViewportSize(size);
    await ["light", "dark"].reduce(async (before, scheme) => {
      await before;
      await page.emulateMedia({ colorScheme: scheme as "light" | "dark" });
      await seen?.scrollIntoViewIfNeeded();
      await page.waitForTimeout(150);
      await page.screenshot({ path: `${SHOTS}/queue-${name}-${label}-${scheme}.png` });
    }, Promise.resolve());
  }, Promise.resolve());
  await page.setViewportSize(SIZES.desktop);
  await page.emulateMedia({ colorScheme: "light" });
};

const prompt = (page: Page, tool: number) =>
  page.locator("dialog[open][data-kind]").filter({ hasText: `Please insert tool ${tool}` });

const resumeAtPrompt = async (page: Page, tool: number) => {
  await expect(prompt(page, tool)).toBeVisible({ timeout: 20_000 });
  await prompt(page, tool).getByRole("button", { name: "Resume" }).click();
};

const addFromOpen = async (page: Page, name: string, skip = false) => {
  await page.getByRole("radio", { name }).click();
  await page.getByRole("checkbox", { name: /Don't stop for bit changes/ }).setChecked(skip);
  await page.getByRole("button", { name: "Add to queue" }).click();
  await expect(page.getByText(`Added ${name} to the queue.`)).toBeVisible();
};

/** Whether the first file is cutting, or its own M6 asks for the bit. */
const started = async (request: APIRequestContext) => {
  const { status, gcode } = await (await request.get("/api/main")).json();
  const past = status.uploadFlag === 1 && gcode.lineIndex > 3;
  return status.manualZAxisAdjust ? "prompt" : past ? "cutting" : "waiting";
};

const closeQueue = (page: Page) =>
  page.getByRole("dialog", { name: "Job Queue" }).getByRole("button", { name: "Close", exact: true }).last().click();

test("queued files run back to back, pausing only for a bit the files show to differ", async ({ page, request }) => {
  test.setTimeout(240_000);
  await Promise.all(Object.entries(FILES).map(([name, text]) => upload(request, name, text)));
  await page.goto("/files/open");
  await waitForServer(page);
  await page.getByLabel("Directory").selectOption({ label: "Top level" });
  await ["queue-a.nc", "queue-b.nc", "queue-c.nc"].reduce(async (previous, name) => {
    await previous;
    await addFromOpen(page, name);
  }, Promise.resolve());
  await addFromOpen(page, "queue-d.nc", true);

  await page.goto("/");
  await chooseMenu(page, "G-Code", "Job Queue");
  const list = page.getByRole("list", { name: "Queued files" });
  await expect(list.getByRole("listitem")).toHaveCount(4);
  await expect(list.getByRole("listitem").nth(2)).toContainText("Tool 2 — will pause for a tool change");
  await expect(list.getByRole("listitem").nth(3)).toContainText("bit change skipped (override)");
  await shoot(page, "idle");

  // The bit in the spindle is unknown, so the first file starts at once; its own T1 M6 may still ask for tool 1.
  await page.getByRole("button", { name: "Start queue" }).click();
  await expect.poll(() => started(request), { timeout: 20_000 }).not.toBe("waiting");
  if ((await started(request)) === "prompt") await resumeAtPrompt(page, 1);
  await expect(page.getByText("Running queue-a.nc.").first()).toBeVisible({ timeout: 20_000 });
  await shoot(page, "started");
  await closeQueue(page);
  await shoot(page, "control-cutting", page.getByRole("region", { name: "Job queue" }));

  await expect.poll(async () => (await queueState(request)).step, { timeout: 60_000 }).toBe("toolChange");
  await closeServerDialogs(page);
  await page.locator(".run-buttons").getByRole("button", { name: "Stop", exact: true }).click();
  const summary = page.getByRole("region", { name: "Job queue" });
  await expect(summary.getByRole("status")).toHaveText("Queue stopped: Stop was pressed.");
  await shoot(page, "control-stopped", summary);

  // Stop at the prompt keeps tool 1 as the bit, so Continue asks for tool 2 again.
  await summary.getByRole("button", { name: "Continue queue" }).click();
  await resumeAtPrompt(page, 2);
  await expect.poll(async () => (await queueState(request)).items.length, { timeout: 30_000 }).toBe(0);
  expect((await queueState(request)).state).toBe("idle");
  await expect(summary).toBeHidden();
});

test("Continue after a Stop part way asks where to start again", async ({ page, request }) => {
  test.setTimeout(120_000);
  await page.goto("/files/open");
  await waitForServer(page);
  await page.getByLabel("Directory").selectOption({ label: "Top level" });
  await addFromOpen(page, "queue-a.nc");
  await page.goto("/");
  await chooseMenu(page, "G-Code", "Job Queue");
  await page.getByRole("button", { name: "Start queue" }).click();
  await closeQueue(page);
  const summary = page.getByRole("region", { name: "Job queue" });
  // The last file named tool 3, so tool 1 is asked for first.
  await resumeAtPrompt(page, 1);
  await expect.poll(() => started(request), { timeout: 20_000 }).toBe("cutting");
  await page.waitForTimeout(3_000);
  await closeServerDialogs(page);
  await page.locator(".run-buttons").getByRole("button", { name: "Stop", exact: true }).click();
  const from = summary.getByRole("button", { name: /^From line \d+ \(where it stopped\)$/ });
  await expect(from).toBeVisible();
  await expect(summary.getByRole("button", { name: "From the start (line 1)" })).toBeVisible();
  await shoot(page, "control-stopped-mid-file", summary);
  await page.waitForTimeout(500);
  await from.click();
  await expect.poll(async () => (await queueState(request)).items.length, { timeout: 60_000 }).toBe(0);
});
