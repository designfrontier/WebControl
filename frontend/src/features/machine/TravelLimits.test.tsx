import { screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeFetch, jsonResponse, startupState } from "../../test/fakeFetch";
import { renderApp } from "../../test/render";
import { maslowSection, setting } from "../settings/fixtures";
import { machineSetup } from "./fixtures";
import { limitText, zInMm } from "./zAxis";

const limits = (upper: string, lower: string) =>
  maslowSection({
    settings: [
      setting({ key: "zAxisUpperLimit", title: "Z-Axis Upper Limit in MM", value: upper, numeric: true }),
      setting({ key: "zAxisLowerLimit", title: "Z-Axis Lower Limit in MM", value: lower, numeric: true }),
    ],
  });

const position = (z: number) => ({
  xval: 0,
  yval: 0,
  zval: z,
  vel: 0,
  rqvel: 0,
  pcom: "0%",
  state: "Idle",
  lpwr: 0,
  rpwr: 0,
  zpwr: 0,
});

const renderLimits = (units: "MM" | "INCHES" = "MM", over: Record<string, unknown> = {}) => {
  const puts: unknown[] = [];
  let current = limits("12.5", "");
  const fetch = fakeFetch({
    "GET /api/state": startupState({ units }),
    "/api/machine/setup": machineSetup(),
    "GET /api/settings/maslow": () => jsonResponse(current),
    "PUT /api/settings/maslow": (init) => {
      const { values } = JSON.parse(String(init?.body)) as { values: Record<string, string> };
      puts.push(values);
      current = limits(values.zAxisUpperLimit ?? "12.5", values.zAxisLowerLimit ?? "");
      return jsonResponse(current);
    },
    ...over,
  });
  return { ...renderApp("/machine/z-limits", { fetch }), puts };
};

const shown = (term: string) => screen.getByText(term).nextElementSibling;

describe("TravelLimits", () => {
  it("shows the stored limits and sets each from the reported Z in mm", async () => {
    const { socket, user, puts } = renderLimits();
    await waitFor(() => expect(shown("Upper limit")).toHaveTextContent("12.5 mm"));
    expect(shown("Lower limit")).toHaveTextContent("Not set");
    expect(shown("Z now")).toHaveTextContent("Unknown");
    expect(screen.getByRole("button", { name: "Set lower limit here" })).toBeDisabled();

    socket.serverMessage("positionMessage", position(-8.25));
    expect(shown("Z now")).toHaveTextContent("-8.25 mm");
    await user.click(screen.getByRole("button", { name: "Set lower limit here" }));
    await waitFor(() => expect(shown("Lower limit")).toHaveTextContent("-8.25 mm"));
    expect(puts).toEqual([{ zAxisLowerLimit: "-8.25" }]);
  });

  it("converts a Z reported in inches", async () => {
    const { socket, user, puts } = renderLimits("INCHES");
    socket.serverMessage("positionMessage", position(0.5));
    await user.click(await screen.findByRole("button", { name: "Set upper limit here" }));
    await waitFor(() => expect(shown("Upper limit")).toHaveTextContent("12.7 mm"));
    expect(puts).toEqual([{ zAxisUpperLimit: "12.7" }]);
  });

  it("says when a limit was not saved", async () => {
    const { socket, user } = renderLimits("MM", {
      "PUT /api/settings/maslow": () => jsonResponse({ errors: { zAxisUpperLimit: "Must be a number" } }, 400),
    });
    socket.serverMessage("positionMessage", position(1));
    await user.click(await screen.findByRole("button", { name: "Set upper limit here" }));
    expect(await screen.findByText(/The limit was not saved/)).toHaveTextContent("failed with 400");
  });

  it("formats limits and converts units", () => {
    expect(limitText(undefined)).toBe("Not set");
    expect(limitText(" ")).toBe("Not set");
    expect(limitText("-3")).toBe("-3 mm");
    expect(zInMm(-0.123, "INCHES")).toBe("-3.124");
    expect(zInMm(4, "MM")).toBe("4");
  });
});
