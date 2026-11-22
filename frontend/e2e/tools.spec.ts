import { expect, test } from "@playwright/test";
import type { APIRequestContext, Locator, Page } from "@playwright/test";
import { closeServerDialogs, connectBackend, waitForServer } from "./helpers";
import { saveSettings, simulatorURL } from "./simulator";

const SHOTS = process.env.E2E_TOOL_SHOTS ?? "e2e/screens";
const VIEWPORTS = {
  desktop: { width: 1280, height: 800 },
  ipad: { width: 1180, height: 820 },
  phone: { width: 390, height: 844 },
};
const FILE = ["G21", "G90", "T11 M6", "G0 X10 Y10", "T16 M6", "G0 X20"];

test.use({ baseURL: simulatorURL, viewport: VIEWPORTS.desktop });
test.describe.configure({ mode: "serial" });
test.skip(({ isMobile }) => isMobile, "the phone layout is shot from the desktop run");

const clearLibrary = async (request: APIRequestContext) => {
  const { tools } = (await (await request.get("/api/tools")).json()) as { tools: { number: number }[] };
  await Promise.all(tools.map(({ number }) => request.delete(`/api/tools/${number}`)));
};

const stop = async () => {
  const socket = await connectBackend(simulatorURL);
  socket.emit("action", { data: { command: "stopRun", arg: "", arg1: "" } });
  socket.disconnect();
};

/** Each viewport in light and dark, with `focus` scrolled into view. */
const shoot = async (page: Page, name: string, focus: Locator) => {
  const shots = Object.entries(VIEWPORTS).flatMap(([label, size]) =>
    (["light", "dark"] as const).map((scheme) => ({ label, size, scheme })),
  );
  await shots.reduce(async (previous, { label, size, scheme }) => {
    await previous;
    await page.emulateMedia({ colorScheme: scheme });
    await page.setViewportSize(size);
    await focus.scrollIntoViewIfNeeded();
    await page.waitForTimeout(300);
    await page.screenshot({ path: `${SHOTS}/${name}-${label}-${scheme}.png` });
  }, Promise.resolve());
  await page.emulateMedia({ colorScheme: "light" });
  await page.setViewportSize(VIEWPORTS.desktop);
};

test.beforeAll(async ({ request }) => {
  await stop();
  await clearLibrary(request);
  await saveSettings(request, "maslow", { zAxis: true });
});

test.afterAll(stop);

test("Settings > Tools adds a tool and follows edits from elsewhere", async ({ page, request }) => {
  await page.goto("/settings/tools");
  await waitForServer(page);
  await closeServerDialogs(page);
  await page.getByRole("button", { name: "Add tool" }).click();
  const form = page.getByRole("form", { name: "New tool" });
  await form.getByLabel("Tool number").fill("11");
  await form.getByLabel("Description").fill('1/4" 2 flute endmill');
  await form.getByLabel("Diameter", { exact: true }).fill("6.35");
  await form.getByLabel("Flutes").fill("2");
  await form.getByRole("button", { name: "Add tool" }).click();
  const library = page.getByRole("list", { name: "Tool library" });
  await expect(library).toContainText('1/4" 2 flute endmill6.35 mm · 2 flutes');

  const added = await request.post("/api/tools", {
    data: { number: 3, description: "60° V-bit", diameter: 12.7, units: "mm", notes: "Engraving only" },
  });
  expect(added.status()).toBe(201);
  await expect(library.getByRole("listitem")).toHaveCount(2);
  await shoot(page, "settings-tools", library);
});

test("the tool-change prompt names the bit from the library", async ({ page, request }) => {
  await request.post("/api/files/gcode/directories", { data: { name: "tools" } });
  const file = { name: "two-tools.nc", mimeType: "text/plain", buffer: Buffer.from(`${FILE.join("\n")}\n`) };
  const upload = await request.post("/uploadGCode", { multipart: { selectedDirectory: "tools", "file[]": file } });
  expect(upload.status()).toBe(200);

  await page.goto("/");
  await waitForServer(page);
  await closeServerDialogs(page);
  await page.getByRole("region", { name: "Run" }).getByRole("button", { name: "Play" }).click();

  const prompt = page.getByRole("dialog", { name: "Notification" });
  await expect(prompt).toContainText('Insert Tool 11 — 1/4" 2 flute endmill', { timeout: 15_000 });
  await expect(prompt).not.toContainText("Please insert tool");
  await shoot(page, "tool-change-prompt", prompt);
  await prompt.getByRole("button", { name: "Resume" }).click();
  await expect(prompt).toContainText("Insert Tool 16 (not in your tool library)", { timeout: 15_000 });
});
