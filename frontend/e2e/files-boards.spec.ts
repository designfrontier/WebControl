import { expect, test } from "@playwright/test";

const gcode = (lines: string[]) => ({
  name: "e2e-part.nc",
  mimeType: "text/plain",
  buffer: Buffer.from(lines.join("\n")),
});

test("uploads, opens, edits and saves gcode through the classic routes", async ({ page }) => {
  await page.goto("/next/files/upload");
  await expect(page.getByText(/^Server: connected/)).toBeVisible({ timeout: 15_000 });

  await page.getByLabel("New directory").fill("e2e jobs");
  await page.getByRole("button", { name: "Create directory" }).click();
  await expect(page.getByLabel("Directory", { exact: true })).toHaveValue("e2e_jobs");

  await page.getByLabel("Gcode files").setInputFiles(gcode(["G21", "G0 X10 Y10", "G1 Z-1", "G1 X20"]));
  await page.getByRole("button", { name: "Upload and open" }).click();
  await expect(page).toHaveURL(/\/next\/?$/);

  await page.goto("/next/files");
  const loaded = page.getByRole("region", { name: "Loaded gcode" });
  await expect(loaded).toContainText("e2e_jobs/e2e-part.nc");
  await expect(loaded).toContainText("4 lines");

  await loaded.getByRole("link", { name: "View or edit…" }).click();
  const editor = page.getByRole("textbox", { name: "Loaded gcode" });
  await expect(editor).toHaveValue(/G0 X10 Y10/);
  await editor.press("ControlOrMeta+End");
  await editor.pressSequentially("\nG0 X0 Y0");
  await page.getByRole("button", { name: "Apply changes" }).click();
  await expect(page).toHaveURL(/\/next\/?$/);

  await page.goto("/next/files");
  await expect(loaded).toContainText("5 lines");

  await loaded.getByRole("link", { name: "Save as…" }).click();
  await page.getByLabel("File name").fill("e2e-part-edited.nc");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page).toHaveURL(/\/next\/files$/);

  await page.getByRole("link", { name: "Open…" }).click();
  await page.getByLabel("Directory", { exact: true }).selectOption("e2e_jobs");
  await expect(page.getByRole("radio", { name: "e2e-part-edited.nc" })).toBeVisible();
  await page.getByRole("radio", { name: "e2e-part.nc" }).check();
  await page.getByRole("button", { name: "Open" }).click();
  await expect(page).toHaveURL(/\/next\/?$/);

  await page.goto("/next/files");
  await expect(loaded).toContainText("4 lines");
  await loaded.getByRole("button", { name: "Clear gcode" }).click();
  await expect(loaded).toContainText("No gcode is loaded.");
});

test("edits, trims and saves the current board", async ({ page }) => {
  // Editing a board resizes it without resizing its cut record, which trimming indexes; start from a clear one.
  await page.goto("/next/boards");
  await page.getByRole("button", { name: "Clear cut data…" }).click();
  await page.getByRole("button", { name: "Clear cut data", exact: true }).click();
  await expect(page.getByRole("button", { name: "Clear cut data…" })).toBeVisible();

  await page.getByRole("link", { name: "Create or edit…" }).click();
  await page.getByLabel("Board ID").fill("E2E shelf");
  await page.getByLabel("Width").fill("1000");
  await page.getByLabel("Height").fill("500");
  await page.getByRole("button", { name: "Center", exact: true }).click();
  await page.getByRole("button", { name: "Save board" }).click();
  await expect(page).toHaveURL(/\/next\/boards$/);

  const current = page.getByRole("region", { name: "Current board" });
  await expect(current).toContainText("E2E shelf");
  await expect(current).toContainText(/Width1000 (mm|in)/);

  await current.getByRole("link", { name: "Trim…" }).click();
  await page.getByLabel("Trim left").fill("100");
  await page.getByRole("button", { name: "Trim board" }).click();
  await expect(page).toHaveURL(/\/next\/boards$/);
  // Boards are stored in inches to two places, so a metric trim may come back a hair short.
  await expect(current).toContainText(/Width(900|899\.9\d) (mm|in)/);

  await current.getByRole("link", { name: "Save as…" }).click();
  await page.getByLabel("File name").fill("e2e-shelf.brd");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(current).toContainText("e2e-shelf.brd");
});
