import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { MachineState } from "../../state/machine";
import { fakeFetch } from "../../test/fakeFetch";
import { boardDetails, expectPath, pending, recorder } from "../../test/filesBoards";
import { renderApp } from "../../test/render";

const setup = (edit = recorder(), board = boardDetails(), machine: Partial<MachineState> = {}) => ({
  ...renderApp("/boards/edit", {
    fetch: fakeFetch({ "/api/boards/current": board, "POST /editBoard": edit.respond }),
    machine,
  }),
  edit,
});

const field = (label: string) => screen.getByLabelText(label);

describe("Create or edit board", () => {
  it("fills in the current board with each measurement's unit", async () => {
    setup(recorder(), boardDetails({ units: "mm", width: 1219.2 }));
    expect(await screen.findByLabelText("Board ID")).toHaveValue("Shelf");
    expect(field("Material")).toHaveValue("Plywood");
    expect(field("Width")).toHaveValue("1219.2");
    expect(field("Width")).toHaveAccessibleDescription("In mm");
    expect(field("Width")).toHaveAttribute("inputmode", "decimal");
    expect(field("Board ID")).not.toHaveAttribute("inputmode");
  });

  it("posts the edited board with its units and returns to the boards screen", async () => {
    const { user, edit } = setup();
    await user.clear(await screen.findByLabelText("Board ID"));
    await user.type(field("Board ID"), "Bench top");
    await user.clear(field("Thickness"));
    await user.type(field("Thickness"), "1.5");
    await user.click(screen.getByRole("button", { name: "Save board" }));

    expect(edit.forms()).toEqual([
      {
        boardID: "Bench top",
        material: "Plywood",
        width: "48",
        height: "24",
        thickness: "1.5",
        centerX: "0",
        centerY: "0",
        units: "inches",
      },
    ]);
    await expectPath("/");
  });

  it("flags measurements that are missing, not numbers or not positive", async () => {
    const { user, edit } = setup();
    await user.clear(await screen.findByLabelText("Width"));
    await user.clear(field("Height"));
    await user.type(field("Height"), "-2");
    await user.clear(field("Center X"));
    await user.type(field("Center X"), "abc");
    await user.click(screen.getByRole("button", { name: "Save board" }));

    expect(screen.getByRole("alert")).toHaveTextContent("Fix the highlighted fields.");
    expect(field("Width")).toHaveAccessibleDescription("Enter a number.");
    expect(field("Width")).toBeInvalid();
    expect(field("Height")).toHaveAccessibleDescription("Must be more than zero.");
    expect(field("Center X")).toHaveAccessibleDescription("Enter a number.");
    expect(field("Center Y")).toBeValid();
    expect(edit.calls).toEqual([]);

    await user.type(field("Width"), "10");
    expect(field("Width")).toBeValid();
  });

  it.each([
    ["Top left", "34", "-7"],
    ["Center", "10", "5"],
    ["Bottom", "0", "17"],
    ["Right", "-14", "0"],
  ])("aligns the %s of the board to the router", async (button, centerX, centerY) => {
    const { user } = setup();
    expect(await screen.findByText(/now at X 10, Y 5\./)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: button }));
    expect(field("Center X")).toHaveValue(centerX);
    expect(field("Center Y")).toHaveValue(centerY);
  });

  it("aligns to the live router position when the machine reports one", async () => {
    const position = {
      x: 1,
      y: 2,
      z: 0,
      velocity: 0,
      requestedVelocity: 0,
      percentComplete: "0",
      state: "Idle",
      power: { left: 0, right: 0, z: 0 },
    };
    const { user } = setup(recorder(), boardDetails(), { position });
    await user.click(await screen.findByRole("button", { name: "Center" }));
    expect(field("Center X")).toHaveValue("1");
    expect(field("Center Y")).toHaveValue("2");
  });

  it("needs a size before aligning", async () => {
    const { user } = setup();
    await user.clear(await screen.findByLabelText("Height"));
    await user.click(screen.getByRole("button", { name: "Top left" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Enter the width and height before aligning.");
    expect(field("Center X")).toHaveValue("0");

    await user.type(field("Height"), "24");
    await user.click(screen.getByRole("button", { name: "Top left" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("explains a failed save and shows progress", async () => {
    const { user } = setup(recorder(500));
    await user.click(await screen.findByRole("button", { name: "Save board" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not update the board.");
  });

  it("disables saving while the server works", async () => {
    const { user } = setup({ respond: pending } as unknown as ReturnType<typeof recorder>);
    await user.click(await screen.findByRole("button", { name: "Save board" }));
    expect(screen.getByRole("button", { name: "Saving…" })).toBeDisabled();
  });
});
