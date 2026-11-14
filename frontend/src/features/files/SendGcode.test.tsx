import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeFetch } from "../../test/fakeFetch";
import { expectPath, recorder } from "../../test/filesBoards";
import { renderApp } from "../../test/render";

const page = () => within(screen.getByRole("region", { name: /gcode/ }));

describe("Send gcode", () => {
  const setup = (send = recorder(), text = "G0 X5") => {
    const fetch = fakeFetch({ "/api/files/gcode/custom": { text }, "POST /sendGCode": send.respond });
    return { ...renderApp("/files/send", { fetch }), send };
  };

  it("starts from the last gcode sent and reports what it sent", async () => {
    const { user, send } = setup();
    const editor = await screen.findByLabelText("Gcode to send");
    expect(editor).toHaveValue("G0 X5");
    expect(editor).toHaveAccessibleDescription("1 line. Each line goes straight to the controller.");

    await user.type(editor, "\nG0 Y5\n");
    await user.click(screen.getByRole("button", { name: "Send" }));

    expect(send.forms()).toEqual([{ gcode: "G0 X5\nG0 Y5" }]);
    expect(await page().findByText("Sent 2 lines to the controller.")).toBeInTheDocument();
    await expectPath("/files/send");

    await user.type(editor, "M3");
    expect(page().queryByText(/^Sent/)).not.toBeInTheDocument();
  });

  it("says one line in the singular", async () => {
    const { user } = setup();
    await user.click(await screen.findByRole("button", { name: "Send" }));
    expect(await page().findByText("Sent 1 line to the controller.")).toBeInTheDocument();
  });

  it("needs something to send", async () => {
    const { user, send } = setup(recorder(), "");
    await user.type(await screen.findByLabelText("Gcode to send"), "  ");
    await user.click(screen.getByRole("button", { name: "Send" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Enter the gcode to send.");
    expect(send.calls).toEqual([]);
  });

  it("explains a failed send", async () => {
    const { user } = setup(recorder(500));
    await user.click(await screen.findByRole("button", { name: "Send" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not send the gcode.");
  });

  it("finishes back on the files screen", async () => {
    const { user } = setup();
    await user.click(await screen.findByRole("link", { name: "Done" }));
    await expectPath("/files");
  });
});
