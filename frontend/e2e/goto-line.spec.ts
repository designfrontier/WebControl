import { expect, test } from "@playwright/test";
import { chooseMenu, resetMachine, waitForServer } from "./helpers";

const TARGET = "G1 X42 Y7";
const FILE = ["﻿(header)", "", "G21", "", "", "; a comment", "G0 X1", "", "(cut)", TARGET, "G1 X43"];

test.beforeEach(({ baseURL }) => resetMachine(baseURL!));

// Upstream #51: View/Edit G-code's line 500 was Go to line 499.
test("Go to line N shows View/Edit G-code's line N", async ({ page }) => {
  await page.goto("/");
  await waitForServer(page);
  const file = { name: "goto-line.nc", mimeType: "text/plain", buffer: Buffer.from(FILE.join("\r\n")) };
  const reply = await page.request.post("/uploadGCode", { multipart: { selectedDirectory: ".", "file[]": file } });
  expect(reply.status()).toBe(200);

  await chooseMenu(page, "G-Code", "View/Edit");
  const dialog = page.getByRole("dialog", { name: "View/Edit G-Code" });
  const editor = dialog.getByRole("textbox", { name: "Loaded gcode" });
  await expect(editor).toHaveValue(new RegExp(TARGET));
  const lines = (await editor.inputValue()).split("\n");
  const line = lines.findIndex((text) => text.trim() === TARGET) + 1;
  expect(lines[0].trim(), "comment lines are kept and numbered").toBe("(header)");
  await dialog.getByRole("button", { name: "Close", exact: true }).click();

  const run = page.getByRole("region", { name: "Run" });
  await run.getByLabel("Line #").fill(String(line));
  await run.getByRole("button", { name: "Go to line" }).click();
  await expect(page.getByRole("status", { name: "Current gcode line" })).toHaveText(new RegExp(`^${TARGET}\\s*$`));
  const readout = page.getByRole("definition").locator("..").filter({ hasText: /^Line/ });
  await expect(readout.getByRole("definition")).toHaveText(String(line));
});
