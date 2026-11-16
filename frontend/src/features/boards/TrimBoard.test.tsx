import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeFetch } from "../../test/fakeFetch";
import { boardDetails, expectPath, recorder } from "../../test/filesBoards";
import { renderApp } from "../../test/render";

const setup = (trim = recorder(), board = boardDetails()) => ({
  ...renderApp("/boards/trim", {
    fetch: fakeFetch({ "/api/boards/current": board, "POST /trimBoard": trim.respond }),
  }),
  trim,
});

const field = (label: string) => screen.getByLabelText(label);
const enter = async (user: ReturnType<typeof setup>["user"], label: string, value: string) => {
  await user.clear(field(label));
  if (value) await user.type(field(label), value);
};

describe("Trim board", () => {
  it("starts with no trim and states the board size", async () => {
    setup(recorder(), boardDetails({ units: "mm", width: 1219.2, height: 609.6 }));
    expect(await screen.findByText(/The board is 1219.2 × 609.6 mm\./)).toBeInTheDocument();
    ["Trim top", "Trim left", "Trim right", "Trim bottom"].forEach((label) => expect(field(label)).toHaveValue("0"));
    expect(field("Trim top")).toHaveAccessibleDescription("In mm");
  });

  it("posts the trims with the board's units and returns to the boards screen", async () => {
    const { user, trim } = setup();
    await screen.findByLabelText("Trim top");
    await enter(user, "Trim top", "1.5");
    await enter(user, "Trim right", "2");
    await user.click(screen.getByRole("button", { name: "Trim board" }));

    expect(trim.forms()).toEqual([{ trimTop: "1.5", trimBottom: "0", trimLeft: "0", trimRight: "2", units: "inches" }]);
    await expectPath("/");
  });

  it("posts a decimal comma trim as a point", async () => {
    const { user, trim } = setup();
    await screen.findByLabelText("Trim top");
    await enter(user, "Trim top", "1,5");
    await user.click(screen.getByRole("button", { name: "Trim board" }));
    expect(trim.forms()).toEqual([{ trimTop: "1.5", trimBottom: "0", trimLeft: "0", trimRight: "0", units: "inches" }]);
  });

  it("rejects blank and negative trims", async () => {
    const { user, trim } = setup();
    await screen.findByLabelText("Trim top");
    await enter(user, "Trim left", "");
    await enter(user, "Trim bottom", "-1");
    await user.click(screen.getByRole("button", { name: "Trim board" }));

    expect(screen.getByRole("alert")).toHaveTextContent("Fix the highlighted fields.");
    expect(field("Trim left")).toHaveAccessibleDescription("Enter a number.");
    expect(field("Trim bottom")).toHaveAccessibleDescription("Cannot be negative.");
    expect(trim.calls).toEqual([]);
  });

  it.each([
    ["Trim left", "Trim right", "30", "18", "width"],
    ["Trim top", "Trim bottom", "20", "4", "height"],
  ])("refuses to trim away the whole board (%s + %s)", async (first, second, a, b, dimension) => {
    const { user, trim } = setup();
    await screen.findByLabelText("Trim top");
    await enter(user, first, a);
    await enter(user, second, b);
    await user.click(screen.getByRole("button", { name: "Trim board" }));
    expect(screen.getByRole("alert")).toHaveTextContent(`must be less than the board's ${dimension}`);
    expect(trim.calls).toEqual([]);
  });

  it("refuses metric trims that the server's rounding to 0.01 in would make the whole width", async () => {
    const { user, trim } = setup(recorder(), boardDetails({ units: "mm", width: 609.6, height: 1219.2 }));
    await screen.findByLabelText("Trim top");
    await enter(user, "Trim left", "304.7");
    await enter(user, "Trim right", "304.8");
    await user.click(screen.getByRole("button", { name: "Trim board" }));
    expect(screen.getByRole("alert")).toHaveTextContent("must be less than the board's width");
    expect(trim.calls).toEqual([]);
  });

  it("explains a failed trim", async () => {
    const { user } = setup(recorder(500));
    await user.click(await screen.findByRole("button", { name: "Trim board" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not trim the board. If you resized it, clear its cut data first.",
    );
    await expectPath("/boards/trim");
  });
});
