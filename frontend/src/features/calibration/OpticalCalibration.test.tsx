import { screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { MachineState } from "../../state/machine";
import { fakeFetch, jsonResponse } from "../../test/fakeFetch";
import { renderApp } from "../../test/render";
import { opticalSettings, opticalState } from "./fixtures";

const setup = ({
  fetch = fakeFetch({ "/api/calibration/optical": opticalState() }),
  machine,
}: { fetch?: ReturnType<typeof fakeFetch>; machine?: Partial<MachineState> } = {}) => {
  const rendered = renderApp("/calibration/optical", { fetch, machine });
  return { ...rendered, fetch, ready: screen.findByRole("form", { name: "Optical calibration settings" }) };
};

const field = (name: string) => screen.getByRole("textbox", { name });
const button = (name: string) => screen.getByRole("button", { name });
const status = () => screen.getByRole("status", { name: "Calibration status" });

describe("optical calibration", () => {
  it("fills the form from the stored settings", async () => {
    const { ready } = setup({
      fetch: fakeFetch({ "/api/calibration/optical": opticalState({ settings: opticalSettings({ scaleX: 1.0028 }) }) }),
    });
    await ready;
    expect(field("Horizontal scale factor")).toHaveValue("1.0028");
    expect(screen.getByRole("combobox", { name: "Calibration extents" })).toHaveValue("Full Area");
    expect(screen.getByRole("combobox", { name: "Gaussian blur" })).toHaveValue("5");
    expect(field("X of top left")).toBeDisabled();
  });

  it("reports when the settings cannot be loaded", async () => {
    setup({ fetch: fakeFetch({ "/api/calibration/optical": () => jsonResponse({}, 500) }) });
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not load the optical calibration settings.");
  });

  it("fills preset extents and unlocks the corners for Custom", async () => {
    const { user, ready } = setup();
    await ready;
    const extents = screen.getByRole("combobox", { name: "Calibration extents" });
    await user.selectOptions(extents, "Top Right");
    expect(field("X of top left")).toHaveValue("0");
    expect(field("X of bottom right")).toHaveValue("15");
    expect(field("Y of bottom right")).toHaveValue("0");

    await user.selectOptions(extents, "Custom");
    expect(field("X of top left")).toBeEnabled();
    await user.clear(field("X of top left"));
    await user.type(field("X of top left"), "-4");
    expect(field("X of top left")).toHaveValue("-4");
  });

  it("saves the settings and shows what the server stored", async () => {
    let saved: unknown;
    const fetch = fakeFetch({
      "/api/calibration/optical": opticalState(),
      "PUT /api/calibration/optical/settings": (init) => {
        saved = JSON.parse(String(init?.body));
        return jsonResponse({ settings: opticalSettings({ cannyLowValue: 42, autoScanDirection: 1 }) });
      },
    });
    const { user, ready } = setup({ fetch });
    await ready;
    await user.clear(field("Canny low value"));
    await user.type(field("Canny low value"), "42");
    await user.selectOptions(screen.getByRole("combobox", { name: "Auto scan direction" }), "1");
    await user.selectOptions(screen.getByRole("combobox", { name: "Gaussian blur" }), "9");
    await user.click(button("Save settings"));

    await waitFor(() => expect(status()).toHaveTextContent("Settings saved."));
    expect(saved).toEqual(opticalSettings({ cannyLowValue: 42, autoScanDirection: 1, gaussianBlurValue: 9 }));
    expect(screen.getByRole("combobox", { name: "Gaussian blur" })).toHaveValue("5");
  });

  it("shows why a save failed", async () => {
    const fetch = fakeFetch({
      "/api/calibration/optical": opticalState(),
      "PUT /api/calibration/optical/settings": () => jsonResponse({ error: "Missing markerY." }, 400),
    });
    const { user, ready } = setup({ fetch });
    await ready;
    await user.click(button("Save settings"));
    await waitFor(() => expect(status()).toHaveTextContent("Missing markerY."));
  });

  it("refuses to save or use the camera with invalid settings", async () => {
    const { user, ready, socket, fetch } = setup();
    await ready;
    await user.clear(field("Vertical size of marker"));
    await user.click(button("Save settings"));
    await user.click(button("Test"));

    expect(field("Vertical size of marker")).toHaveAccessibleDescription("Enter a number");
    expect(status()).toHaveTextContent("Check these settings: Vertical size of marker.");
    expect(socket.emittedEvents("opticalCalibration")).toEqual([]);
    expect(fetch.mock.calls.some(([, init]) => init?.method === "PUT")).toBe(false);
  });

  it("runs the camera actions with the current settings", async () => {
    const { user, ready, socket } = setup();
    await ready;
    await user.clear(field("Position tolerance"));
    await user.type(field("Position tolerance"), "0.2");
    await user.click(button("Test"));
    expect(status()).toHaveTextContent("Asked for a test image.");
    await user.click(button("Find center"));
    await user.click(button("Start automatic calibration"));

    const sent = socket.emittedEvents("opticalCalibration").map(({ data }) => data.command);
    expect(sent).toEqual(["testOpticalCalibration", "findCenterOpticalCalibration", "optical_Calibrate"]);
    expect(socket.emittedEvents("opticalCalibration")[0].data.arg).toEqual(opticalSettings({ positionTolerance: 0.2 }));
  });

  it("puts the center the camera found into the form", async () => {
    const { ready, socket } = setup();
    await ready;
    socket.serverMessage("updateOpticalCalibrationFindCenter", { opticalCenterX: 321.456, opticalCenterY: 239.5 });
    expect(field("X coordinate of center")).toHaveValue("321.46");
    expect(field("Y coordinate of center")).toHaveValue("239.50");
    expect(status()).toHaveTextContent("Found the center.");
  });

  it("shows the test and calibration images as they arrive", async () => {
    const { user, ready, socket } = setup();
    await ready;
    expect(screen.getByText("Test image: none yet.")).toBeInTheDocument();
    await user.click(button("Test"));
    socket.serverMessage("updateCalibrationImage", { command: "OpticalCalibrationTestImageUpdated", data: "VEVTVA==" });
    expect(status()).toHaveTextContent("Test image updated.");
    socket.serverMessage("updateCalibrationImage", { command: "OpticalCalibrationImageUpdated", data: "QUNUSVZF" });
    expect(screen.getByRole("img", { name: "Test image" })).toHaveAttribute("src", "data:image/png;base64,VEVTVA==");
    expect(screen.getByRole("img", { name: "Latest calibration image" })).toHaveAttribute(
      "src",
      "data:image/png;base64,QUNUSVZF",
    );
  });

  it("sends the plain calibration actions", async () => {
    const { user, ready, socket } = setup();
    await ready;
    const labels = [
      "▲ Up 5 mm",
      "▼ Down 5 mm",
      "Stop",
      "(Re)load calibration",
      "Return to center",
      "Save to CSV",
      "Save and send",
      "Do curve fit",
    ];
    await labels.reduce((previous, label) => previous.then(() => user.click(button(label))), Promise.resolve());
    expect(socket.actions()).toEqual([
      ["adjustCenter", 5, ""],
      ["adjustCenter", -5, ""],
      ["stopOpticalCalibration", "", ""],
      ["reloadCalibration", "", ""],
      ["returnToCenter", "", ""],
      ["saveCalibrationToCSV", "", ""],
      ["saveAndSendOpticalCalibration", "", ""],
      ["curveFitOpticalCalibration", "", ""],
    ]);
  });

  it("redraws the charts when new curve and error data arrive, and clears them", async () => {
    const { user, ready, socket } = setup();
    await ready;
    const xCurve = () => screen.getByRole("img", { name: "X curve: 15 lines" });
    const flat = xCurve().innerHTML;

    socket.serverMessage("updateOpticalCalibrationCurve", {
      curveX: [1, 0.01, 0, 0, 0, 0],
      curveY: [0, 0, 0, 0, 0, 0],
    });
    expect(xCurve().innerHTML).not.toBe(flat);

    const errorX = Array.from({ length: 31 }, () => Array.from({ length: 15 }, () => 0.5));
    socket.serverMessage("updateOpticalCalibrationError", { errorX, errorY: errorX });
    expect(screen.getByRole("img", { name: "XY error: 465 grid points" })).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "X error: 15 lines" }).textContent).toContain("1.5");

    await user.click(button("Clear calibration"));
    expect(socket.actions()).toEqual([["clearCalibration", "", ""]]);
    expect(xCurve().innerHTML).toBe(flat);
    expect(screen.getByRole("img", { name: "X error: 15 lines" }).textContent).not.toContain("1.5");
  });

  it("tracks the sled in inches", async () => {
    const position = {
      x: 254,
      y: -127,
      z: 0,
      velocity: 0,
      requestedVelocity: 0,
      percentComplete: "",
      state: "Idle",
      power: { left: 0, right: 0, z: 0 },
    };
    const { ready } = setup({ machine: { units: "MM", position } });
    await ready;
    expect(screen.getByRole("img", { name: "Sled position: X 10.00 in, Y -5.00 in" })).toBeInTheDocument();
  });

  it("shows an unknown sled position before the first report", async () => {
    const { ready } = setup();
    await ready;
    expect(screen.getByRole("img", { name: "Sled position: position unknown" })).toBeInTheDocument();
  });
});
