import { expect, test } from "@playwright/test";
import type { Browser, Page } from "@playwright/test";
import { connectBackend, resetMachine, waitForServer } from "./helpers";

const FOLDER = "mancala";
const NAME = "mancala.frd.roughing.25.endmill.nc";

const IPADS_LANDSCAPE = [
  { width: 1024, height: 768 },
  { width: 1180, height: 820 },
  { width: 1366, height: 1024 },
];

const runFile = (page: Page) => page.getByRole("region", { name: "Run" }).getByText("File:").locator("..");
const viewerFile = (page: Page) => page.getByRole("figure").getByText("File:").locator("..");

const openTablet = async (browser: Browser, viewport: { width: number; height: number }) => {
  const context = await browser.newContext({ viewport, hasTouch: true });
  const page = await context.newPage();
  await page.goto("/");
  await waitForServer(page);
  return page;
};

const upload = async (page: Page) => {
  await page.request.post("/api/files/gcode/directories", { data: { name: FOLDER } });
  const file = { name: NAME, mimeType: "text/plain", buffer: Buffer.from("G21\nG0 X10 Y10\nG1 Z-1\nG1 X20\n") };
  const reply = await page.request.post("/uploadGCode", { multipart: { selectedDirectory: FOLDER, "file[]": file } });
  expect(reply.status()).toBe(200);
};

test.beforeEach(({ baseURL }) => resetMachine(baseURL!));

test("every open screen names the loaded file as it is loaded and cleared", async ({ browser, baseURL }, testInfo) => {
  test.skip(testInfo.project.name !== "desktop", "opens its own tablet and desktop viewports");
  const tablet = await openTablet(browser, IPADS_LANDSCAPE[0]);
  const desktop = await openTablet(browser, { width: 1280, height: 800 });
  await Promise.all([tablet, desktop].map((page) => expect(runFile(page)).toHaveText("File: No file loaded")));

  await upload(desktop);
  await Promise.all(
    [tablet, desktop].map(async (page) => {
      await expect(runFile(page)).toContainText(`File: ${NAME}`);
      await expect(runFile(page)).toHaveAttribute("title", `${FOLDER}/${NAME}`);
      await expect(viewerFile(page)).toContainText(`${NAME} in ${FOLDER}`);
    }),
  );

  const socket = await connectBackend(baseURL!);
  socket.emit("action", { data: { command: "clearGCode", arg: "", arg1: "" } });
  socket.disconnect();
  await Promise.all([tablet, desktop].map((page) => expect(runFile(page)).toHaveText("File: No file loaded")));
});

test("on a landscape iPad the file name sits beside Play, whole and inside the first screen", async ({
  browser,
}, testInfo) => {
  test.skip(testInfo.project.name !== "desktop", "opens its own tablet viewports");
  const check = async (viewport: { width: number; height: number }) => {
    const page = await openTablet(browser, viewport);
    if (viewport === IPADS_LANDSCAPE[0]) await upload(page);
    await expect(runFile(page)).toContainText(NAME);
    await expect(runFile(page)).toBeInViewport({ ratio: 1 });
    await expect(page.getByRole("button", { name: "Play" })).toBeInViewport({ ratio: 1 });
    const whole = await runFile(page).evaluate(
      (line) => line.scrollWidth <= line.clientWidth && line.scrollHeight <= line.clientHeight,
    );
    expect(whole, `the name is cut off at ${viewport.width}x${viewport.height}`).toBe(true);
    expect(await page.evaluate(() => document.documentElement.scrollHeight)).toBeLessThanOrEqual(viewport.height);
    await page.context().close();
  };
  await IPADS_LANDSCAPE.reduce((previous, viewport) => previous.then(() => check(viewport)), Promise.resolve());
});
