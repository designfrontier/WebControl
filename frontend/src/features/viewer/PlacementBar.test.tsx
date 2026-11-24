import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { RunState } from "../../state/machine";
import { PlacementBar } from "./PlacementBar";

const bar = (over: { runState?: RunState; homeLocked?: boolean; probing?: boolean; fits?: boolean } = {}) =>
  render(
    <PlacementBar
      home={{ x: 1.5, y: -2 }}
      units="INCHES"
      fits={over.fits ?? true}
      runState={over.runState ?? "idle"}
      homeLocked={over.homeLocked ?? false}
      probing={over.probing ?? false}
      onConfirm={() => {}}
      onCancel={() => {}}
    />,
  );

describe("PlacementBar", () => {
  it("shows the new home and lets it be set", () => {
    bar();
    expect(screen.getByRole("group", { name: "Position gcode" })).toHaveTextContent("Home X 1.50, Y -2.00 in");
    expect(screen.getByRole("button", { name: "Set home here" })).toBeEnabled();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it.each([
    [{ probing: true }, "Unavailable while the probe runs."],
    [{ runState: "running" as const }, "Unavailable while gcode is running."],
    [{ runState: "paused" as const }, "Setting home is unavailable while a run is paused."],
    [{ homeLocked: true }, "Stop the job before setting home."],
  ])("refuses to set home when the server would: %o", (gates, reason) => {
    bar(gates);
    expect(screen.getByText(reason)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Set home here" })).toBeDisabled();
  });

  it("warns when the toolpath runs off the bed", () => {
    bar({ fits: false });
    expect(screen.getByRole("alert")).toHaveTextContent("Part of the toolpath is off the bed.");
  });
});
