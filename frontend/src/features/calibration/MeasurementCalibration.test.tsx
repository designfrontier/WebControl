import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeFetch, jsonResponse } from "../../test/fakeFetch";
import { renderApp } from "../../test/render";

const current = { motorYoffset: 463, rotationRadius: 139.1, chainSagCorrection: 33 };
const calculated = {
  motorYoffset: 400.6,
  rotationRadius: 138.8,
  chainSagCorrection: 33.443549,
  calibrationError: -0.1358,
};

const status = () => screen.getByRole("status", { name: "Calibration status" });
const results = () => within(screen.getByRole("table", { name: "Machine parameters" }));
const row = (name: string) =>
  within(results().getByRole("rowheader", { name }).closest("tr")!)
    .getAllByRole("cell")
    .map((cell) => cell.textContent);

const triangular = (routes = {}) => {
  const fetch = fakeFetch({
    "/api/calibration/triangular": current,
    "POST /api/calibration/triangular": calculated,
    ...routes,
  });
  return { ...renderApp("/calibration/triangular", { fetch }), fetch };
};

describe("triangular calibration", () => {
  it("shows the current parameters before anything is calculated", async () => {
    triangular();
    expect(screen.getByRole("heading", { name: "Triangular calibration" })).toBeInTheDocument();
    await waitFor(() => expect(row("Height of motors")).toEqual(["463", "—"]));
    expect(row("Chain sag correction")).toEqual(["33", "—"]);
    expect(screen.getByRole("button", { name: "Accept results" })).toBeDisabled();
  });

  it("cuts the pattern and stops it", async () => {
    const { user, socket } = triangular();
    await user.click(screen.getByRole("button", { name: "Cut calibration pattern" }));
    await user.click(screen.getByRole("button", { name: "Stop" }));
    expect(socket.actions()).toEqual([
      ["cutTriangularCalibrationPattern", "", ""],
      ["stopRun", "", ""],
    ]);
  });

  it("calculates from the measurements, then accepts the results", async () => {
    const { user, socket, fetch } = triangular();
    const cut5 = screen.getByRole("textbox", { name: "Top of work area to cut 5" });
    await user.clear(cut5);
    await user.type(cut5, "310.5");
    await user.click(screen.getByRole("button", { name: "Calculate" }));

    await waitFor(() => expect(row("Height of motors")).toEqual(["463", "400.6"]));
    expect(row("Calibration error")).toEqual(["—", "-0.1358"]);
    const post = fetch.mock.calls.find(([, init]) => init?.method === "POST");
    expect(JSON.parse(String(post?.[1]?.body))).toEqual({ cut12: 1930, cut34: 1930, cut5: 310.5, bitDiameter: 6.35 });
    expect(status()).toHaveTextContent("Review the calculated values");

    await user.click(screen.getByRole("button", { name: "Accept results" }));
    expect(socket.actions()).toEqual([["acceptTriangularCalibrationResults", 0, 0]]);
    expect(status()).toHaveTextContent("Accepted.");
    expect(row("Height of motors")).toEqual(["400.6", "—"]);
    expect(screen.getByRole("button", { name: "Accept results" })).toBeDisabled();
  });

  it("flags fields that are not numbers without asking the server", async () => {
    const { user, fetch } = triangular();
    const field = screen.getByRole("textbox", { name: "Router bit diameter" });
    await user.clear(field);
    await user.type(field, "quarter inch");
    await user.click(screen.getByRole("button", { name: "Calculate" }));

    expect(field).toHaveAttribute("aria-invalid", "true");
    expect(field).toHaveAccessibleDescription("Enter a number");
    expect(status()).toHaveTextContent("Check the highlighted measurements.");
    expect(fetch.mock.calls.some(([, init]) => init?.method === "POST")).toBe(false);
  });

  it("shows the server's reason when the calculation is refused", async () => {
    const { user } = triangular({
      "POST /api/calibration/triangular": () =>
        jsonResponse({ error: "The bit diameter of 30.0 mm seems wrong." }, 400),
    });
    await user.click(screen.getByRole("button", { name: "Calculate" }));
    await waitFor(() => expect(status()).toHaveTextContent("The bit diameter of 30.0 mm seems wrong."));
    expect(status()).toHaveAttribute("data-tone", "error");
    expect(screen.getByRole("button", { name: "Accept results" })).toBeDisabled();
  });

  it("says so when the current settings cannot be loaded", async () => {
    triangular({ "/api/calibration/triangular": () => jsonResponse({}, 500) });
    expect(await screen.findByText("Could not load the current settings.")).toBeInTheDocument();
  });
});

describe("holey calibration", () => {
  it("sends the twelve measurements in order and shows the fitted parameters", async () => {
    const fetch = fakeFetch({
      "/api/calibration/holey": {
        motorYoffset: 463,
        distanceBetweenMotors: 2978.4,
        leftChainTolerance: 0,
        rightChainTolerance: 0,
      },
      "POST /api/calibration/holey": {
        motorYoffset: 468.4,
        distanceBetweenMotors: 3601.2,
        leftChainTolerance: 0.12,
        rightChainTolerance: -0.08,
        calibrationError: 1.5,
      },
    });
    const { user, socket } = renderApp("/calibration/holey", { fetch });
    expect(screen.getAllByRole("textbox")).toHaveLength(12);
    await user.clear(screen.getByRole("textbox", { name: "M12" }));
    await user.type(screen.getByRole("textbox", { name: "M12" }), "254");
    await user.click(screen.getByRole("button", { name: "Calculate" }));

    await waitFor(() => expect(row("Distance between motors")).toEqual(["2978.4", "3601.2"]));
    expect(row("Right chain tolerance")).toEqual(["0", "-0.08"]);
    const post = fetch.mock.calls.find(([, init]) => init?.method === "POST");
    expect(JSON.parse(String(post?.[1]?.body))).toEqual({ measurements: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 254] });

    await user.click(screen.getByRole("button", { name: "Cut calibration pattern" }));
    await user.click(screen.getByRole("button", { name: "Accept results" }));
    expect(socket.actions()).toEqual([
      ["cutHoleyCalibrationPattern", "", ""],
      ["acceptHoleyCalibrationResults", 0, 0],
    ]);
  });
});
