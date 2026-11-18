import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { fakeFetch, jsonResponse, startupState } from "../../test/fakeFetch";
import { renderApp } from "../../test/render";
import { errorGrid, opticalFit, opticalSession, opticalSettings, opticalState } from "./opticalFixtures";

const OPTICAL = "/api/calibration/optical";
type Routes = Parameters<typeof fakeFetch>[0];

const setup = ({ routes = {}, firmware = 0 }: { routes?: Routes; firmware?: number } = {}) => {
  const startup = startupState({ controllerFirmwareVersion: firmware });
  const fetch = fakeFetch({ "GET /api/state": startup, [OPTICAL]: opticalState(), ...routes });
  const rendered = renderApp("/calibration/optical", { fetch });
  return { ...rendered, fetch, ready: screen.findByRole("form", { name: "Optical calibration settings" }) };
};

const field = (name: string) => screen.getByRole("textbox", { name });
const button = (name: string) => screen.getByRole("button", { name });
const status = () => screen.getByRole("status", { name: "Calibration status" });
const bodyOf = (fetch: ReturnType<typeof fakeFetch>, call: string) => {
  const [method, path] = call.split(" ");
  const found = fetch.mock.calls.find(([input, init]) => input === path && (init?.method ?? "GET") === method);
  return found ? JSON.parse(String(found[1]?.body)) : undefined;
};
const calls = (fetch: ReturnType<typeof fakeFetch>, path: string) =>
  fetch.mock.calls.filter(([input, init]) => input === path && (init?.method ?? "GET") === "GET").length;
const progress = (over: object = {}) => ({
  state: "moving",
  cell: { i: -15, j: 7 },
  attempt: 1,
  done: 0,
  total: 465,
  ...over,
});

describe("optical calibration visibility", () => {
  const enabled = () =>
    fakeFetch({ "GET /api/state": startupState({ experimental: ["optical"] }), [OPTICAL]: opticalState() });

  it("is not listed while the setting is off", async () => {
    const fetch = fakeFetch();
    renderApp("/calibration", { fetch });
    await waitFor(() => expect(calls(fetch, "/api/state")).toBeGreaterThan(1));
    const methods = within(screen.getByRole("navigation", { name: "Calibration methods" }));
    expect(methods.getAllByRole("link").map((link) => link.textContent)).toEqual(["Triangular", "Holey"]);
    expect(screen.queryByRole("link", { name: "Open optical calibration" })).not.toBeInTheDocument();
  });

  it("stays unlisted when the state cannot be loaded", async () => {
    const fetch = fakeFetch({ "GET /api/state": () => jsonResponse({}, 500) });
    const warn = vi.spyOn(console, "warn").mockImplementation(() => undefined);
    renderApp("/calibration", { fetch });
    await waitFor(() => expect(calls(fetch, "/api/state")).toBeGreaterThan(1));
    expect(screen.queryByRole("link", { name: "Open optical calibration" })).not.toBeInTheDocument();
    warn.mockRestore();
  });

  it("is listed in the calibration page while the setting is on", async () => {
    const { user } = renderApp("/calibration", { fetch: enabled() });
    await user.click(await screen.findByRole("link", { name: "Open optical calibration" }));
    expect(await screen.findByRole("heading", { name: "Optical calibration" })).toBeInTheDocument();
    const methods = within(screen.getByRole("navigation", { name: "Calibration methods" }));
    expect(methods.getByRole("link", { name: "Optical" })).toHaveAttribute("aria-current", "page");
  });

  it("is in the Actions setup links only while the setting is on", async () => {
    const off = fakeFetch();
    const first = renderApp("/actions", { fetch: off });
    await waitFor(() => expect(calls(off, "/api/state")).toBeGreaterThan(1));
    expect(await screen.findByRole("link", { name: "Triangular calibration" })).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Optical calibration (experimental)" })).not.toBeInTheDocument();
    first.unmount();

    renderApp("/actions", { fetch: enabled() });
    expect(await screen.findByRole("link", { name: "Optical calibration (experimental)" })).toHaveAttribute(
      "href",
      "/calibration/optical",
    );
  });

  it("shows the server's reason at its address while the setting is off", async () => {
    const off = "Optical calibration is turned off. Turn it on in Settings > WebControl Settings.";
    setup({ routes: { [OPTICAL]: () => jsonResponse({ error: off }, 409) } });
    expect(await screen.findByRole("alert")).toHaveTextContent(off);
    expect(screen.getByRole("link", { name: "Printable target (Letter)" })).toBeInTheDocument();
  });

  it("falls back to a generic message when the server gives no reason", async () => {
    setup({ routes: { [OPTICAL]: () => new Response("oops", { status: 500 }) } });
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not load optical calibration.");
  });
});

describe("optical calibration screen", () => {
  it("is labelled experimental and says what the result does on holey firmware", async () => {
    setup({ firmware: 51.29 });
    const note = screen.getByRole("note", { name: "Experimental" });
    expect(note).toHaveTextContent("Experimental");
    expect(note).toHaveTextContent("The measurement alone corrects nothing.");
    await waitFor(() => expect(note).toHaveTextContent("holey firmware 51.29, which uses all four"));
  });

  it.each([
    [0, "No controller has reported its firmware yet."],
    [1.26, "stock firmware 1.26; the fit models holey firmware"],
    [101.25, "custom firmware 101.25; the fit models holey firmware"],
  ])("explains the result for firmware %s", async (firmware, text) => {
    setup({ firmware });
    await waitFor(() => expect(screen.getByRole("note", { name: "Experimental" })).toHaveTextContent(text));
  });

  it("links the printable targets and the CSV", async () => {
    const { ready } = setup();
    await ready;
    expect(screen.getByRole("link", { name: "Printable target (Letter)" })).toHaveAttribute(
      "href",
      `${OPTICAL}/target.pdf?paper=letter`,
    );
    expect(screen.getByRole("link", { name: "Printable target (A4)" })).toHaveAttribute(
      "href",
      `${OPTICAL}/target.pdf?paper=a4`,
    );
    expect(screen.getByRole("link", { name: "Download CSV" })).toHaveAttribute("href", `${OPTICAL}/csv`);
  });

  it("fills the form from the stored settings", async () => {
    const { ready } = setup({
      routes: { [OPTICAL]: opticalState({ settings: opticalSettings({ scaleX: 1.0028, feedRate: 750 }) }) },
    });
    await ready;
    expect(field("Horizontal scale factor")).toHaveValue("1.0028");
    expect(field("Feed rate between squares")).toHaveValue("750");
    expect(field("Settle time")).toHaveValue("1");
    expect(field("X of top left")).toBeDisabled();
  });

  it("fills preset extents and unlocks the corners for Custom", async () => {
    const { user, ready } = setup();
    await ready;
    const extents = screen.getByRole("combobox", { name: "Calibration extents" });
    await user.selectOptions(extents, "Top Right");
    expect(field("X of top left")).toHaveValue("0");
    expect(field("Y of bottom right")).toHaveValue("0");
    await user.selectOptions(extents, "Custom");
    expect(field("X of top left")).toBeEnabled();
  });

  it("saves the settings and shows what the server stored", async () => {
    const { user, ready, fetch } = setup({
      routes: { [`PUT ${OPTICAL}/settings`]: { settings: opticalSettings({ cannyLowValue: 42, settleTime: 2 }) } },
    });
    await ready;
    await user.clear(field("Settle time"));
    await user.type(field("Settle time"), "2");
    await user.clear(field("Canny low value"));
    await user.type(field("Canny low value"), "41");
    await user.selectOptions(screen.getByRole("combobox", { name: "Auto scan direction" }), "1");
    await user.selectOptions(screen.getByRole("combobox", { name: "Gaussian blur" }), "9");
    await user.click(button("Save settings"));
    await waitFor(() => expect(status()).toHaveTextContent("Settings saved."));
    expect(bodyOf(fetch, `PUT ${OPTICAL}/settings`)).toEqual(
      opticalSettings({ cannyLowValue: 41, settleTime: 2, autoScanDirection: 1, gaussianBlurValue: 9 }),
    );
    expect(field("Canny low value")).toHaveValue("42");
  });

  it("shows why a save failed", async () => {
    const { user, ready } = setup({
      routes: { [`PUT ${OPTICAL}/settings`]: () => jsonResponse({ error: "Missing markerY." }, 400) },
    });
    await ready;
    await user.click(button("Save settings"));
    await waitFor(() => expect(status()).toHaveTextContent("Missing markerY."));
  });

  it("refuses to save, scan or use the camera with invalid settings", async () => {
    const { user, ready, fetch } = setup();
    await ready;
    await user.clear(field("Vertical size of marker"));
    await user.click(button("Save settings"));
    await user.click(button("Start scan"));
    await user.click(button("Test image"));
    await user.click(button("Find centre"));
    expect(field("Vertical size of marker")).toHaveAccessibleDescription("Enter a number");
    expect(status()).toHaveTextContent("Check these settings: Vertical size of marker.");
    expect(fetch.mock.calls.filter(([, init]) => init?.method && init.method !== "GET")).toEqual([]);
  });
});

describe("optical camera", () => {
  it("asks for the camera state and toggles the live view", async () => {
    const { user, ready, socket } = setup();
    await ready;
    expect(screen.getByText("synthetic")).toBeInTheDocument();
    expect(screen.getByText("Live view is off.")).toBeInTheDocument();
    await user.click(button("Start live view"));
    expect(socket.actions()).toContainEqual(["queryCamera", "", ""]);
    expect(socket.actions()).toContainEqual(["toggleCamera", "", ""]);

    socket.serverMessage("cameraMessage", { command: "updateCamera", data: "on" });
    socket.serverMessage("cameraMessage", { command: "cameraImageUpdated", data: "TElWRQ==" });
    expect(screen.getByRole("img", { name: "Live camera view" })).toHaveAttribute(
      "src",
      "data:image/png;base64,TElWRQ==",
    );
    expect(button("Stop live view")).toBeInTheDocument();

    socket.serverMessage("cameraMessage", { command: "updateCamera", data: "off" });
    expect(screen.queryByRole("img", { name: "Live camera view" })).not.toBeInTheDocument();
    socket.serverMessage("cameraMessage", { command: "updateCamera", data: "on" });
    expect(screen.getByText("Live view is off.")).toBeInTheDocument();
  });

  it("says when the configured camera is missing", async () => {
    const { ready } = setup({
      routes: { [OPTICAL]: opticalState({ camera: { source: "auto", available: false } }) },
    });
    await ready;
    expect(screen.getByText(/not found; set Camera Source/)).toBeInTheDocument();
  });

  it("captures a test image with the form's settings and reports the square", async () => {
    const measurement = { dxMm: 1.234, dyMm: -0.5, centrePx: { x: 330, y: 250 }, angleDeg: 2.04, framesUsed: 10 };
    const { user, ready, fetch } = setup({
      routes: { [`POST ${OPTICAL}/test-image`]: { png: "VEVTVA==", measurement } },
    });
    await ready;
    await user.clear(field("Position tolerance"));
    await user.type(field("Position tolerance"), "0.2");
    await user.click(button("Test image"));
    await waitFor(() =>
      expect(status()).toHaveTextContent("Found the square 1.23 mm right and -0.50 mm up of the centre, turned 2.0°."),
    );
    expect(screen.getByRole("img", { name: "Test image" })).toHaveAttribute("src", "data:image/png;base64,VEVTVA==");
    expect(bodyOf(fetch, `POST ${OPTICAL}/test-image`)).toEqual({
      settings: opticalSettings({ positionTolerance: 0.2 }),
    });
  });

  it("reports a test image with no square, and a failed capture", async () => {
    let answer = jsonResponse({ png: "VEVTVA==", measurement: null });
    const { user, ready } = setup({ routes: { [`POST ${OPTICAL}/test-image`]: () => answer } });
    await ready;
    await user.click(button("Test image"));
    await waitFor(() => expect(status()).toHaveTextContent("No square found in the test image."));
    answer = jsonResponse({ error: "No camera or no frame" }, 422);
    await user.click(button("Test image"));
    await waitFor(() => expect(status()).toHaveTextContent("No camera or no frame"));
  });

  it("puts the centre it found into the form, and reports a failure", async () => {
    let answer = jsonResponse({ opticalCenterX: 321.456, opticalCenterY: 239.5 });
    const { user, ready } = setup({ routes: { [`POST ${OPTICAL}/find-center`]: () => answer } });
    await ready;
    await user.click(button("Find centre"));
    await waitFor(() => expect(field("X coordinate of center")).toHaveValue("321.46"));
    expect(field("Y coordinate of center")).toHaveValue("239.50");
    expect(status()).toHaveTextContent("Found the centre. Save the settings to keep it.");
    answer = jsonResponse({ error: "No square" }, 422);
    await user.click(button("Find centre"));
    await waitFor(() => expect(status()).toHaveTextContent("No square"));
  });
});

describe("optical scan", () => {
  it("starts a scan with the form's settings", async () => {
    const { user, ready, fetch } = setup({ routes: { [`POST ${OPTICAL}/scan`]: { total: 465 } } });
    await ready;
    await user.click(button("Start scan"));
    await waitFor(() => expect(status()).toHaveTextContent("Scan started: 465 squares."));
    expect(bodyOf(fetch, `POST ${OPTICAL}/scan`)).toEqual({ settings: opticalSettings() });
  });

  it("replaces Scan started with how the scan ended", async () => {
    const { user, ready, socket } = setup({ routes: { [`POST ${OPTICAL}/scan`]: { total: 465 } } });
    await ready;
    await user.click(button("Start scan"));
    await waitFor(() => expect(status()).toHaveTextContent("Scan started: 465 squares."));
    socket.serverMessage("opticalScanProgress", progress({ state: "moving", done: 3 }));
    expect(status()).toHaveTextContent("Scan started: 465 squares.");

    socket.serverMessage("opticalScanProgress", progress({ state: "failed", done: 3 }));
    expect(status()).toHaveTextContent("Scan waiting on a failed square after 3 of 465 squares.");
    expect(status()).toHaveAttribute("data-tone", "error");
    socket.serverMessage("opticalScanProgress", progress({ state: "measuring", done: 3 }));
    expect(status()).toHaveTextContent("Scan resumed after 3 of 465 squares.");
    socket.serverMessage("opticalScanProgress", progress({ state: "stopped", done: 4 }));
    expect(status()).toHaveTextContent("Scan stopped after 4 of 465 squares.");

    await user.click(button("Start scan"));
    await waitFor(() => expect(status()).toHaveTextContent("Scan started: 465 squares."));
    socket.serverMessage("opticalScanProgress", progress({ state: "done", cell: null, done: 465 }));
    expect(status()).toHaveTextContent("Scan done: 465 of 465 squares.");
    expect(status()).toHaveAttribute("data-tone", "ok");
    expect(screen.getByRole("progressbar", { name: "Scan progress" })).toHaveClass("scan-progress");
  });

  it("shows why a scan could not start", async () => {
    const { user, ready } = setup({
      routes: { [`POST ${OPTICAL}/scan`]: () => jsonResponse({ error: "A job is running." }, 409) },
    });
    await ready;
    await user.click(button("Start scan"));
    await waitFor(() => expect(status()).toHaveTextContent("A job is running."));
  });

  it("follows the progress and reloads the grid as squares are recorded", async () => {
    const { ready, socket, fetch } = setup();
    await ready;
    const map = () => screen.getByRole("img", { name: /^Error map/ });
    expect(map()).toHaveAccessibleName(/0 of 465 squares measured/);
    expect(screen.getByText("Idle")).toBeInTheDocument();
    const loads = calls(fetch, OPTICAL);

    socket.serverMessage("opticalScanProgress", progress({ state: "moving" }));
    expect(screen.getByText("Moving: column -15, row 7, attempt 1")).toBeInTheDocument();
    expect(button("Start scan")).toBeDisabled();
    await waitFor(() => expect(calls(fetch, OPTICAL)).toBe(loads + 1));

    socket.serverMessage("opticalScanProgress", progress({ state: "measuring" }));
    expect(screen.getByRole("progressbar", { name: "Scan progress" })).toHaveAttribute("value", "0");
    expect(calls(fetch, OPTICAL)).toBe(loads + 1);

    const recorded = opticalSession({
      state: "moving",
      errorX: errorGrid((i, j) => (i === -15 && j === 7 ? 0.3 : null)),
      errorY: errorGrid((i, j) => (i === -15 && j === 7 ? -0.4 : null)),
    });
    fetch.mockImplementationOnce(async () => jsonResponse(opticalState({ session: recorded })));
    socket.serverMessage("opticalScanProgress", progress({ state: "moving", cell: { i: -14, j: 7 }, done: 1 }));
    await waitFor(() => expect(map()).toHaveAccessibleName(/1 of 465 squares measured, largest error 0.50 mm/));
    expect(within(map()).getByText("column -15, row 7: X 0.30 mm, Y -0.40 mm")).toBeInTheDocument();
    expect(within(map()).getByText("column -14, row 7: not measured").parentElement).toHaveClass("heat-current");
  });

  it("offers Retry, Skip and Stop on a failed square", async () => {
    const { user, ready, socket } = setup();
    await ready;
    socket.serverMessage(
      "opticalScanProgress",
      progress({ state: "failed", cell: { i: 2, j: -3 }, attempt: 6, error: "No square found" }),
    );
    expect(screen.getByRole("alert")).toHaveTextContent("The square at column 2, row -3 failed: No square found");
    expect(
      within(screen.getByRole("img", { name: /^Error map/ })).getByText(/column 2, row -3/).parentElement,
    ).toHaveClass("heat-failed");
    await user.click(button("Retry square"));
    await user.click(button("Skip square"));
    await user.click(button("Stop scan"));
    expect(socket.actions().filter(([command]) => command !== "queryCamera")).toEqual([
      ["opticalRetryCell", "", ""],
      ["opticalSkipCell", "", ""],
      ["stopRun", "", ""],
    ]);
  });

  it("offers Retry and Skip for a scan that was already waiting on a failed square", async () => {
    const { user, ready, socket } = setup({
      routes: { [OPTICAL]: opticalState({ session: opticalSession({ state: "failed" }) }) },
    });
    await ready;
    expect(screen.getByRole("alert")).toHaveTextContent(/^A square failed\./);
    await user.click(button("Skip square"));
    expect(socket.actions()).toContainEqual(["opticalSkipCell", "", ""]);
  });

  it("words a failure with no square or reason", async () => {
    const { ready, socket } = setup();
    await ready;
    socket.serverMessage("opticalScanProgress", progress({ state: "failed", cell: null }));
    expect(screen.getByRole("alert")).toHaveTextContent(/^A square failed\.Retry square/);
  });

  it("shows the camera orientation and warns about a mirrored or turned camera", async () => {
    const { ready, socket } = setup({
      routes: {
        [OPTICAL]: opticalState({
          session: opticalSession({ orientation: { rotationDeg: 1.25, mirrored: false, pxPerMm: 4.7 } }),
        }),
      },
    });
    await ready;
    expect(screen.getByText("Camera turned 1.3°, not mirrored, 4.70 px/mm.")).toBeInTheDocument();
    socket.serverMessage("opticalOrientation", { rotationDeg: 180, mirrored: false, pxPerMm: 4.7 });
    expect(screen.getByText(/Camera turned 180.0°.*remount it\./)).toBeInTheDocument();
    socket.serverMessage("opticalOrientation", { rotationDeg: 0, mirrored: true, pxPerMm: 4.7 });
    expect(screen.getByText(/mirrored, 4.70 px\/mm\. A scan refuses/)).toBeInTheDocument();
  });

  it("tracks the sled on the sheet", async () => {
    const { ready, socket } = setup();
    await ready;
    expect(screen.getByRole("img", { name: "Sled position: position unknown" })).toBeInTheDocument();
    socket.serverMessage("positionMessage", {
      xval: 254,
      yval: -127,
      zval: 0,
      vel: 0,
      rqvel: 0,
      pcom: "",
      state: "Idle",
      lpwr: 0,
      rpwr: 0,
      zpwr: 0,
    });
    expect(await screen.findByRole("img", { name: /^Sled position: X 10.00 in, Y -5.00 in/ })).toBeInTheDocument();
  });

  it("shows the latest scan image", async () => {
    const { ready, socket } = setup();
    await ready;
    socket.serverMessage("opticalImage", { cell: { i: 0, j: 0 }, png: "SU1BR0U=" });
    expect(screen.getByRole("img", { name: "Latest scan image, column 0, row 0" })).toHaveAttribute(
      "src",
      "data:image/png;base64,SU1BR0U=",
    );
  });

  it("sends Stop, Return to centre and Clear measurements", async () => {
    const measured = opticalSession({ state: "done", errorX: errorGrid(() => 0.1), errorY: errorGrid(() => 0) });
    const { user, ready, socket } = setup({ routes: { [OPTICAL]: opticalState({ session: measured }) } });
    await ready;
    expect(screen.getByText("Done")).toBeInTheDocument();
    await user.click(button("Stop"));
    await user.click(button("Return to centre"));
    await user.click(button("Clear measurements"));
    expect(socket.actions().filter(([command]) => command !== "queryCamera")).toEqual([
      ["stopRun", "", ""],
      ["returnToCenter", "", ""],
      ["opticalClearSession", "", ""],
    ]);
    expect(screen.getByRole("img", { name: /^Error map/ })).toHaveAccessibleName(/0 of 465/);
    expect(button("Clear measurements")).toBeDisabled();
  });
});

describe("optical geometry fit", () => {
  const measured = () =>
    opticalState({
      session: opticalSession({ state: "done", errorX: errorGrid(() => 0.2), errorY: errorGrid(() => 0) }),
    });
  const cellsOf = (name: string) =>
    within(
      within(screen.getByRole("table", { name: "Fitted geometry" }))
        .getByRole("rowheader", { name })
        .closest("tr")!,
    )
      .getAllByRole("cell")
      .map((cell) => cell.textContent);

  it("needs measurements before it can fit", async () => {
    const { ready } = setup();
    await ready;
    expect(button("Fit geometry")).toBeDisabled();
    expect(button("Accept…")).toBeDisabled();
  });

  it("shows the fit beside the current values and writes nothing until confirmed", async () => {
    const { user, ready, fetch } = setup({
      routes: {
        [OPTICAL]: measured(),
        [`POST ${OPTICAL}/fit`]: opticalFit(),
        [`POST ${OPTICAL}/accept`]: opticalFit().fitted,
      },
    });
    await ready;
    await user.click(button("Fit geometry"));
    await waitFor(() => expect(cellsOf("Distance between motors")).toEqual(["2978.4", "2981.35", "+2.95"]));
    expect(cellsOf("Height of motors")).toEqual(["463", "458.1", "-4.9"]);
    expect(cellsOf("Right chain tolerance")).toEqual(["0", "-0.08", "-0.08"]);
    expect(screen.getByText(/From 465 squares, RMS residual 0.31 mm/)).toBeInTheDocument();
    expect(status()).toHaveTextContent("Review the fitted values");

    await user.click(button("Accept…"));
    const dialog = within(screen.getByRole("dialog", { name: "Write the fitted geometry?" }));
    expect(dialog.getAllByRole("listitem").map((item) => item.textContent)).toEqual([
      "Distance between motors: 2978.4 → 2981.35",
      "Height of motors: 463 → 458.1",
      "Left chain tolerance: 0 → 0.12",
      "Right chain tolerance: 0 → -0.08",
    ]);
    await user.click(dialog.getByRole("button", { name: "Cancel" }));
    expect(bodyOf(fetch, `POST ${OPTICAL}/accept`)).toBeUndefined();

    await user.click(button("Accept…"));
    await user.click(screen.getByRole("button", { name: "Write settings" }));
    await waitFor(() => expect(status()).toHaveTextContent("Accepted."));
    expect(bodyOf(fetch, `POST ${OPTICAL}/accept`)).toEqual({});
    expect(cellsOf("Distance between motors")).toEqual(["2981.35", "2981.35", "0"]);
    expect(button("Accept…")).toBeDisabled();
  });

  it("shows why a fit or an accept failed", async () => {
    const { user, ready } = setup({
      routes: {
        [OPTICAL]: measured(),
        [`POST ${OPTICAL}/fit`]: opticalFit(),
        [`POST ${OPTICAL}/accept`]: () => jsonResponse({ error: "A job is running." }, 409),
      },
    });
    await ready;
    await user.click(button("Fit geometry"));
    await user.click(await screen.findByRole("button", { name: "Accept…" }));
    await user.click(screen.getByRole("button", { name: "Write settings" }));
    await waitFor(() => expect(status()).toHaveTextContent("A job is running."));
    expect(button("Accept…")).toBeEnabled();
  });

  it("reports a fit that did not converge", async () => {
    const { user, ready } = setup({
      routes: { [OPTICAL]: measured(), [`POST ${OPTICAL}/fit`]: () => jsonResponse({ error: "No fit" }, 422) },
    });
    await ready;
    await user.click(button("Fit geometry"));
    await waitFor(() => expect(status()).toHaveTextContent("No fit"));
    expect(screen.queryByRole("table", { name: "Fitted geometry" })).not.toBeInTheDocument();
  });
});
