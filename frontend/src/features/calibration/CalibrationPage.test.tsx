import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeFetch } from "../../test/fakeFetch";
import { currentPath, renderApp } from "../../test/render";

const fetch = () =>
  fakeFetch({
    "/api/calibration/triangular": { motorYoffset: 463, rotationRadius: 139.1, chainSagCorrection: 33 },
  });

describe("CalibrationPage", () => {
  it("lists the two methods with what each is for", () => {
    renderApp("/calibration", { fetch: fetch() });
    expect(screen.getByRole("heading", { level: 1, name: "Calibration" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Open triangular calibration" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Open holey calibration" })).toBeInTheDocument();
    expect(screen.queryByText(/optical/i)).not.toBeInTheDocument();
  });

  it("moves between methods with the sub-navigation", async () => {
    const { user } = renderApp("/calibration", { fetch: fetch() });
    await user.click(screen.getByRole("link", { name: "Open triangular calibration" }));
    expect(currentPath()).toBe("/calibration/triangular");
    expect(screen.getByRole("heading", { name: "Triangular calibration" })).toBeInTheDocument();

    const methods = within(screen.getByRole("navigation", { name: "Calibration methods" }));
    expect(methods.getByRole("link", { name: "Triangular" })).toHaveAttribute("aria-current", "page");
    await user.click(methods.getByRole("link", { name: "Holey" }));
    expect(currentPath()).toBe("/calibration/holey");
    expect(methods.getAllByRole("link").map((link) => link.textContent)).toEqual(["Triangular", "Holey"]);
  });

  it("explains an unknown method address", () => {
    renderApp("/calibration/laser", { fetch: fetch() });
    expect(screen.getByText("There is no calibration method at this address.")).toBeInTheDocument();
  });
});
