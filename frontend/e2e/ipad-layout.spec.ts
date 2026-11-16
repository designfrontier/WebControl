import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";
import { closeServerDialogs, connectBackend, waitForServer } from "./helpers";
import { simulatorURL } from "./simulator";

const IPADS = [
  { name: "mini landscape", width: 1024, height: 768 },
  { name: "Air landscape", width: 1180, height: 820 },
  { name: "Pro landscape", width: 1366, height: 1024 },
  { name: "mini portrait", width: 768, height: 1024 },
  { name: "Air portrait", width: 820, height: 1180 },
  { name: "Pro portrait", width: 1024, height: 1366 },
];

const FOLDER = "ipad";
const NAME = "mancala.frd.roughing.25.endmill.nc";
/** G17 and G40 each leave a controller message; whichever tool the sled holds, one M6 stops the job to change it. */
const GCODE = [
  "G21",
  "G90",
  "G17",
  "G40",
  "T1 M6",
  "G1 X1 F800",
  "T2 M6",
  ...Array.from({ length: 400 }, (_, i) => `G1 X${(i % 20) * 5} Y${Math.floor(i / 20) * 5} F800`),
].join("\n");
/** The most the layout may lean on: the controls and the message log each keep this much in hand. */
const SPARE_PX = 20;

test.use({ baseURL: simulatorURL, hasTouch: true });
test.skip(({ isMobile }) => isMobile, "sets its own tablet viewports");

const action = async (command: string) => {
  const socket = await connectBackend(simulatorURL);
  socket.emit("action", { data: { command, arg: "", arg1: "" } });
  await new Promise((resolve) => setTimeout(resolve, 300));
  socket.disconnect();
};

/** A job left at a tool change locks the gcode, and uploads with it. */
const reset = async () => {
  await action("stopRun");
  await action("clearGCode");
};
test.beforeEach(reset);
test.afterEach(reset);

const startJob = async (page: Page) => {
  await page.request.post("/api/files/gcode/directories", { data: { name: FOLDER } });
  const file = { name: NAME, mimeType: "text/plain", buffer: Buffer.from(GCODE) };
  const reply = await page.request.post("/uploadGCode", { multipart: { selectedDirectory: FOLDER, "file[]": file } });
  expect(reply.status()).toBe(200);
  await page.goto("/");
  await waitForServer(page);
  await page.getByRole("button", { name: "Play" }).click();
  await expect(page.locator("dialog[open][data-kind]")).toBeVisible({ timeout: 15_000 });
  await closeServerDialogs(page);
  await expect(page.locator(".job-state")).toHaveText("Waiting for tool change");
  await expect(page.getByRole("list", { name: "Status" }).getByText(/^CPU /)).toBeVisible();
  await expect(page.getByRole("log", { name: "Controller Messages" }).getByRole("listitem").nth(1)).toBeAttached();
};

/** Header rows, page overflow, the viewer stage's shape and how much room the controls and the log have left. */
const measure = (page: Page) =>
  page.evaluate(() => {
    const box = (selector: string) => document.querySelector(selector)!.getBoundingClientRect();
    const panel = document.querySelector(".control-panel")!;
    const log = document.querySelector(".console ol")!;
    const logStyle = getComputedStyle(log);
    const twoLines = 2 * parseFloat(logStyle.lineHeight) + parseFloat(logStyle.paddingTop) * 2;
    // The jog pad grows into spare height and gives it back first, down to three rows of the control height.
    const control = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--control"));
    const jogSlack = box(".jog-grid").height - (3 * control + 2 * 4);
    const logSpare = log.clientHeight - twoLines;
    // Where the log runs under the controls, taller controls take its height instead.
    const aboveLog = panel.getBoundingClientRect().bottom <= log.getBoundingClientRect().top ? logSpare : 0;
    const stage = box(".viewer-stage");
    const tops = [".brand", ".nav", ".status-bar"].map((selector) => Math.round(box(selector).top));
    const chips = Array.from(document.querySelectorAll(".status-bar > .chip")).map((chip) =>
      chip.getBoundingClientRect(),
    );
    const chipsWidth = Math.max(...chips.map((chip) => chip.right)) - Math.min(...chips.map((chip) => chip.left));
    const topbar = getComputedStyle(document.querySelector(".topbar")!);
    const menuRowSpare =
      box(".topbar").right - parseFloat(topbar.paddingRight) - box(".nav").right - parseFloat(topbar.columnGap);
    return {
      overflow: [
        document.documentElement.scrollHeight - innerHeight,
        document.documentElement.scrollWidth - innerWidth,
      ],
      header: Math.round(box(".topbar").height),
      oneRow: Math.max(...tops) - Math.min(...tops) < 20,
      headerSpare: Math.round(Math.min(box(".status-bar").width - chipsWidth, menuRowSpare)),
      stageAspect: stage.width / stage.height,
      stageShare: stage.height / innerHeight,
      panelOverflow: panel.scrollHeight - panel.clientHeight,
      controlsSpare: Math.round(Math.max(0, panel.clientHeight - panel.scrollHeight) + jogSlack + aboveLog),
      logSpare: Math.round(logSpare),
      smallTargets: Array.from(
        document.querySelectorAll<HTMLElement>(".shell button, .shell input, .shell select, .shell a[href]"),
      )
        .filter((target) => target.offsetParent !== null && target.getBoundingClientRect().height < 44)
        .map((target) => target.getAttribute("aria-label") ?? target.textContent),
    };
  });

/** The software chip's visible form, which must name the build: every release says 0.1.0. */
const shownSoftware = (page: Page) =>
  page.evaluate(() => {
    const forms = Array.from(document.querySelectorAll('.chip[data-kind="software"] > span'));
    return forms.find((form) => form.getBoundingClientRect().width > 2)?.textContent ?? null;
  });

const expectCommitShown = async (page: Page, size: string) => {
  const { commit } = (await (await page.request.get("/api/state")).json()) as { commit: string };
  expect(commit, "the e2e build reports its commit").toMatch(/^[0-9a-f]{7,}$/);
  await expect(page.locator('.chip[data-kind="software"]'), size).toBeInViewport({ ratio: 1 });
  await expect.poll(() => shownSoftware(page), { message: `build commit on ${size}` }).toContain(commit);
};

test("a job at its tool change fits every iPad without scrolling, with run, status and messages in view", async ({
  page,
}) => {
  await startJob(page);
  const run = page.getByRole("region", { name: "Run" });
  const readouts = page.getByRole("definition").locator("..");
  const inView = [
    run.getByRole("button", { name: "Play" }),
    run.getByRole("button", { name: /^(Pause|Resume)$/ }),
    run.getByRole("button", { name: "Stop", exact: true }),
    run.getByText("File:").locator(".."),
    run.getByRole("group", { name: "Line pacing" }),
    readouts.filter({ hasText: /^Job/ }),
    readouts.filter({ hasText: /^Line/ }),
    readouts.filter({ hasText: /^Complete/ }),
    page.getByRole("log", { name: "Controller Messages" }).getByRole("listitem").nth(1),
    page.getByRole("button", { name: "Up", exact: true }),
    page.getByRole("button", { name: "Down right" }),
  ];

  await IPADS.reduce(async (previous, ipad) => {
    await previous;
    await page.setViewportSize(ipad);
    await expect(run.getByText("File:").locator("..")).toContainText(NAME);
    const portrait = ipad.height > ipad.width;
    const size = `${ipad.name} ${ipad.width}x${ipad.height}`;
    await expect
      .poll(() => measure(page).then((m) => m.overflow), { message: `page scroll on ${size}` })
      .toEqual([0, 0]);
    await inView.reduce(async (done, locator) => {
      await done;
      await expect(locator, size).toBeInViewport({ ratio: 1 });
    }, Promise.resolve());
    await expectCommitShown(page, size);

    const m = await measure(page);
    console.log(size, JSON.stringify(m));
    expect(m.header, `header on ${size}`).toBeLessThanOrEqual(portrait ? 90 : 54);
    expect(m.smallTargets, `touch targets under 44 px on ${size}`).toEqual([]);
    if (!portrait) expect(m.oneRow, `menus and status share a row on ${size}`).toBe(true);
    expect(m.headerSpare, `header room on ${size}`).toBeGreaterThanOrEqual(SPARE_PX);
    expect(m.stageAspect, `viewer stage shape on ${size}`).toBeGreaterThanOrEqual(1.6);
    if (portrait) expect(m.stageShare, `viewer height on ${size}`).toBeLessThanOrEqual(0.4);
    expect(m.panelOverflow, `controls scroll on ${size}`).toBeLessThanOrEqual(0);
    expect(m.logSpare, `message log room on ${size}`).toBeGreaterThanOrEqual(SPARE_PX);
    if (!portrait) expect(m.controlsSpare, `controls room on ${size}`).toBeGreaterThanOrEqual(SPARE_PX);
  }, Promise.resolve());
});

test("Units and Set Home Here look enabled when idle", async ({ page }) => {
  await page.setViewportSize(IPADS[0]);
  await page.goto("/");
  await waitForServer(page);
  const jog = page.getByRole("region", { name: "Jog" });
  const buttons = [jog.getByRole("button", { name: /^Units: / }), jog.getByRole("button", { name: "Set Home Here" })];
  await buttons.reduce(async (previous, button) => {
    await previous;
    await expect(button).toBeEnabled();
    await expect(button).toHaveCSS("opacity", "1");
  }, Promise.resolve());
});

test("the build commit shows beside the firmware on a phone and on desktops", async ({ page }) => {
  await page.goto("/");
  await waitForServer(page);
  await [
    { width: 390, height: 844 },
    { width: 1280, height: 800 },
    { width: 1440, height: 900 },
    { width: 1920, height: 1080 },
  ].reduce(async (previous, viewport) => {
    await previous;
    await page.setViewportSize(viewport);
    await expectCommitShown(page, `${viewport.width}x${viewport.height}`);
  }, Promise.resolve());
});

test("a mouse keeps the cursor readout and compact controls that touch gives up", async ({ page, browser }) => {
  await page.setViewportSize(IPADS[0]);
  await page.goto("/");
  await waitForServer(page);
  await expect(page.getByRole("status", { name: "Cursor position" })).toBeHidden();

  const mouse = await browser.newContext({ baseURL: simulatorURL, viewport: IPADS[0], hasTouch: false });
  const desk = await mouse.newPage();
  await desk.goto("/");
  await waitForServer(desk);
  await expect(desk.getByRole("status", { name: "Cursor position" })).toBeVisible();
  await expect(desk.getByRole("button", { name: "Play" })).toHaveCSS("min-height", "40px");
  await mouse.close();
});
