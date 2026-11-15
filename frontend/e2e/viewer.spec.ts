import { expect, test } from "@playwright/test";
import type { APIRequestContext, Locator } from "@playwright/test";
import { connectBackend, resetMachine } from "./helpers";

const square = (size: number) =>
  [
    "G21",
    "G90",
    "G0 X0 Y0 Z5",
    "G1 Z-1 F500",
    `G1 X${size} Y0`,
    `G1 X${size} Y${size}`,
    `G1 X0 Y${size}`,
    "G1 X0 Y0",
    "G0 Z5",
    "M3 S12000",
  ].join("\n");

const zigzag = (teeth: number) =>
  [
    "G21",
    "G90",
    "G0 X0 Y0 Z5",
    "G1 Z-1 F500",
    ...Array.from({ length: teeth }, (_, i) => `G1 X${(i + 1) * 10} Y${i % 2 ? 0 : 10}`),
    "G0 Z5",
  ].join("\n");

const currentGcode = async (request: APIRequestContext) =>
  ((await (await request.get("/api/viewer")).json()) as { gcode: string | null }).gcode;

/** Uploads through the /uploadGCode form route, then waits for the background parse to replace the compressed preview. */
const uploadGcode = async (request: APIRequestContext, name: string, gcode: string) => {
  const before = await currentGcode(request);
  const response = await request.post("/uploadGCode", {
    multipart: { selectedDirectory: "", "file[]": { name, mimeType: "text/plain", buffer: Buffer.from(gcode) } },
  });
  expect(response.ok()).toBe(true);
  await expect.poll(() => currentGcode(request), { timeout: 15_000 }).not.toBe(before);
};

const openScreenActions = async (canvas: Locator, isMobile: boolean) => {
  const box = (await canvas.boundingBox())!;
  const position = { x: box.width * 0.75, y: box.height / 2 };
  if (!isMobile) return canvas.click({ button: "right", position });
  const at = { pointerType: "touch", isPrimary: true, clientX: box.x + position.x, clientY: box.y + position.y };
  await canvas.dispatchEvent("pointerdown", at);
  await canvas.page().waitForTimeout(1_700);
  await canvas.dispatchEvent("pointerup", at);
};

test.beforeEach(({ baseURL }) => resetMachine(baseURL!));

test("draws uploaded gcode, follows socket broadcasts and defines home from the canvas", async ({
  page,
  request,
  baseURL,
  isMobile,
}) => {
  await uploadGcode(request, "square.nc", square(50));
  await page.goto("/");

  const viewer = page.getByRole("figure", { name: "Gcode and board viewer" });
  await expect(page.getByText(/^Server: connected/)).toBeVisible({ timeout: 15_000 });
  await expect(viewer.locator("canvas")).toBeVisible();
  await expect(viewer).toHaveAttribute("data-cut-segments", /^[1-9]\d*$/);
  const squareCuts = await viewer.getAttribute("data-cut-segments");

  // The gcodeUpdateCompressed broadcast arrives as base64 text over the WebSocket.
  await uploadGcode(request, "zigzag.nc", zigzag(7));
  const socket = await connectBackend(baseURL!);
  try {
    socket.emit("checkForGCodeUpdate", { data: "Please" });
    await expect(viewer).not.toHaveAttribute("data-cut-segments", squareCuts!);
    await expect(page.getByText(/^\d+ cut and \d+ rapid moves$/)).toBeVisible();

    await openScreenActions(viewer.getByRole("img", { name: /Bed, sled and toolpath/ }), isMobile);
    const dialog = page.getByRole("dialog", { name: "Screen action" });
    await expect(dialog).toBeVisible();
    await dialog.getByRole("button", { name: "Define home here" }).click();
    await expect(dialog).toBeHidden();

    await expect
      .poll(
        async () =>
          ((await (await request.get("/api/state")).json()) as { homePosition: { x: number } }).homePosition.x,
      )
      .toBeGreaterThan(0);
  } finally {
    socket.emit("action", { data: { command: "resetHomeToCenter", arg: "", arg1: "" } });
    socket.disconnect();
  }
});
