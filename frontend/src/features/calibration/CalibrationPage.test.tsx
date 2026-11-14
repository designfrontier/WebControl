import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeFetch } from "../../test/fakeFetch";
import { currentPath, renderApp } from "../../test/render";
import { opticalState } from "./fixtures";

const fetch = () =>
  fakeFetch({
    "/api/calibration/triangular": { motorYoffset: 463, rotationRadius: 139.1, chainSagCorrection: 33 },
    "/api/calibration/optical": opticalState(),
  });

describe("CalibrationPage", () => {
  it("lists the three methods with what each is for", () => {
    renderApp("/calibration", { fetch: fetch() });
    expect(screen.getByRole("heading", { level: 1, name: "Calibration" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Triangular calibration" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Holey calibration" })).toBeInTheDocument();
    expect(screen.getByText(/camera on the sled/)).toBeInTheDocument();
  });

  it("moves between methods with the sub-navigation", async () => {
    const { user } = renderApp("/calibration", { fetch: fetch() });
    await user.click(screen.getByRole("link", { name: "Triangular calibration" }));
    expect(currentPath()).toBe("/calibration/triangular");
    expect(screen.getByRole("heading", { name: "Triangular calibration" })).toBeInTheDocument();

    const methods = within(screen.getByRole("navigation", { name: "Calibration methods" }));
    expect(methods.getByRole("link", { name: "Triangular" })).toHaveAttribute("aria-current", "page");
    await user.click(methods.getByRole("link", { name: "Optical" }));
    expect(currentPath()).toBe("/calibration/optical");
    expect(await screen.findByRole("form", { name: "Optical calibration settings" })).toBeInTheDocument();
  });

  it("explains an unknown method address", () => {
    renderApp("/calibration/laser", { fetch: fetch() });
    expect(screen.getByText("There is no calibration method at this address.")).toBeInTheDocument();
  });
});
