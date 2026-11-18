import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";
import { chooseMenu, closeServerDialogs, resetMachine, waitForServer } from "./helpers";

const gcode = (lines: string[]) => ({
  name: "e2e-part.nc",
  mimeType: "text/plain",
  buffer: Buffer.from(lines.join("\n")),
});

const atControl = (page: Page) => expect(page).toHaveURL(/\/$/);

const loadedGcode = async (page: Page) => {
  await chooseMenu(page, "G-Code", "View/Edit");
  const dialog = page.getByRole("dialog", { name: "View/Edit G-Code" });
  const editor = dialog.getByRole("textbox", { name: "Loaded gcode" });
  await expect(editor).not.toHaveValue("");
  const text = await editor.inputValue();
  await dialog.getByRole("button", { name: "Close", exact: true }).click();
  return text;
};

test.beforeEach(({ baseURL }) => resetMachine(baseURL!));

test("uploads, opens, edits, saves and clears gcode from the G-Code menu", async ({ page }) => {
  await page.goto("/files/upload");
  await waitForServer(page);

  await page.getByLabel("New directory").fill("e2e jobs");
  await page.getByRole("button", { name: "Create directory" }).click();
  await expect(page.getByLabel("Directory", { exact: true })).toHaveValue("e2e_jobs");

  await page.getByLabel("Gcode files").setInputFiles(gcode(["G21", "G0 X10 Y10", "G1 Z-1", "G1 X20"]));
  await page.getByRole("button", { name: "Upload and open" }).click();
  await atControl(page);
  await expect(page.getByText(/^\d+ cut and \d+ rapid moves$/)).toBeVisible();

  await chooseMenu(page, "G-Code", "View/Edit");
  const editor = page.getByRole("textbox", { name: "Loaded gcode" });
  await expect(editor).toHaveValue(/G0 X10 Y10/);
  await editor.fill(`${await editor.inputValue()}\nG0 X0 Y0`);
  await page.getByRole("button", { name: "Apply changes" }).click();
  await atControl(page);
  await expect.poll(() => loadedGcode(page)).toMatch(/G0 X0 Y0\s*$/);

  await chooseMenu(page, "G-Code", "Save");
  await page.getByLabel("File name").fill("e2e-part-edited.nc");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await atControl(page);

  await chooseMenu(page, "G-Code", "Open");
  await page.getByLabel("Directory", { exact: true }).selectOption("e2e_jobs");
  await expect(page.getByRole("radio", { name: "e2e-part-edited.nc" })).toBeVisible();
  await page.getByRole("radio", { name: "e2e-part.nc" }).check();
  await page.getByRole("button", { name: "Open", exact: true }).click();
  await atControl(page);
  await expect.poll(() => loadedGcode(page)).toMatch(/G1 X20\s*$/);

  await chooseMenu(page, "G-Code", "Clear GCode");
  await page
    .getByRole("dialog", { name: "Clear the loaded gcode?" })
    .getByRole("button", { name: "Clear GCode" })
    .click();
  await expect(
    page
      .getByRole("figure", { name: "Gcode and board viewer" })
      .getByText(/^(0 cut and 0 rapid moves|No file loaded)$/),
  ).toBeVisible();
  await closeServerDialogs(page);
});

test("blocks the whole page while gcode loads, through Escape, until the server answers", async ({ page }) => {
  await page.goto("/files/upload");
  await waitForServer(page);
  await page.getByLabel("Gcode files").setInputFiles(gcode(["G21", "G0 X5 Y5"]));
  await page.getByRole("button", { name: "Upload and open" }).click();
  await atControl(page);

  let release = () => {};
  await page.route("**/openGCode", (route) => {
    release = () => void route.continue();
  });
  await page.goto("/files/open");
  await page.getByRole("button", { name: "Open", exact: true }).click();
  const overlay = page.getByRole("dialog", { name: "Opening gcode…" });
  await expect(overlay).toBeVisible();

  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await expect(overlay).toBeVisible();
  await expect(page).toHaveURL(/\/files\/open$/);
  const covered = await page.evaluate(() => document.elementFromPoint(5, 5)?.closest(".loading-overlay") != null);
  expect(covered).toBe(true);

  release();
  await atControl(page);
  await expect(overlay).toBeHidden();
});

test("edits, trims and saves the current board from the Boards menu", async ({ page }) => {
  await page.goto("/");
  await waitForServer(page);
  // Editing a board resizes it without resizing its cut record, which trimming indexes; start from a clear one.
  await chooseMenu(page, "Boards", "Clear Board");
  await page
    .getByRole("dialog", { name: "Clear the board's cut data?" })
    .getByRole("button", { name: "Clear Board" })
    .click();

  await chooseMenu(page, "Boards", "Create/Edit Board");
  await page.getByLabel("Board ID").fill("E2E shelf");
  await page.getByLabel("Width").fill("1000");
  await page.getByLabel("Height").fill("500");
  await page.getByRole("button", { name: "Center", exact: true }).click();
  await page.getByRole("button", { name: "Save board" }).click();
  await atControl(page);
  await expect(page.getByRole("button", { name: /^Board: E2E shelf/ })).toBeVisible();

  await chooseMenu(page, "Boards", "Trim Board");
  await page.getByLabel("Trim left").fill("100");
  await page.getByRole("button", { name: "Trim board" }).click();
  await atControl(page);

  await chooseMenu(page, "Boards", "Create/Edit Board");
  // Boards are stored in inches to two places, so a metric trim may come back a hair short.
  await expect(page.getByLabel("Width")).toHaveValue(/^(900|899\.9\d)$/);
  await page.getByRole("button", { name: "Cancel" }).click();

  await chooseMenu(page, "Boards", "Save Board");
  await page.getByLabel("File name").fill("e2e-shelf.brd");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await atControl(page);

  await chooseMenu(page, "Boards", "Open Board");
  await expect(page.getByRole("radio", { name: "e2e-shelf.brd" })).toBeVisible();
});
