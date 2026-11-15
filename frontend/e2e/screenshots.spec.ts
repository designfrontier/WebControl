import { test } from "@playwright/test";
import type { Page } from "@playwright/test";
import { closeServerDialogs, waitForServer } from "./helpers";

// Writes to e2e/screens/ (gitignored) for reviewing the layout by eye; it asserts nothing about pixels.
const shoot = async (page: Page, path: string, name: string) => {
  await page.goto(path);
  await waitForServer(page);
  await closeServerDialogs(page);
  await page.waitForTimeout(500);
  await page.screenshot({ path: `e2e/screens/${name}.png` });
};

test("control screen, light and dark", async ({ page, isMobile }) => {
  const prefix = isMobile ? "phone" : "desktop";
  await page.emulateMedia({ colorScheme: "light" });
  await shoot(page, "/", `${prefix}-control-light`);
  await page.emulateMedia({ colorScheme: "dark" });
  await shoot(page, "/", `${prefix}-control-dark`);
});

test("representative modals", async ({ page, isMobile }) => {
  const prefix = isMobile ? "phone" : "desktop";
  await page.emulateMedia({ colorScheme: "light" });
  await shoot(page, "/settings/advanced", `${prefix}-modal-settings`);
  await shoot(page, "/files/open", `${prefix}-modal-open-gcode`);
  await page.emulateMedia({ colorScheme: "dark" });
  await shoot(page, "/actions", `${prefix}-modal-actions`);
});
