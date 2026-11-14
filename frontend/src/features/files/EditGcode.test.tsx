import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeFetch } from "../../test/fakeFetch";
import { expectPath, recorder } from "../../test/filesBoards";
import { renderApp } from "../../test/render";

const page = () => within(screen.getByRole("region", { name: /gcode/ }));

describe("View or edit gcode", () => {
  const setup = (edit = recorder(), text = "G21\nG0 X1 Y1\nG1 Z-1") => {
    const fetch = fakeFetch({ "/api/files/gcode/text": { text }, "POST /editGCode": edit.respond });
    return { ...renderApp("/files/edit", { fetch }), edit };
  };

  it("shows the loaded gcode in a plain editor", async () => {
    setup();
    const editor = await screen.findByLabelText("Loaded gcode");
    expect(editor).toHaveValue("G21\nG0 X1 Y1\nG1 Z-1");
    expect(editor).toHaveAttribute("spellcheck", "false");
    expect(editor).toHaveAttribute("wrap", "off");
    expect(editor).toHaveAccessibleDescription(/^3 lines\. Applying replaces the loaded gcode/);
    expect(screen.getByRole("button", { name: "Apply changes" })).toBeDisabled();
  });

  it("applies edits without trailing blank lines and returns to the control screen", async () => {
    const { user, edit } = setup();
    const editor = await screen.findByLabelText("Loaded gcode");
    await user.type(editor, "\nG0 X0{Enter}{Enter}");
    await user.click(screen.getByRole("button", { name: "Apply changes" }));

    expect(edit.forms()).toEqual([{ gcode: "G21\nG0 X1 Y1\nG1 Z-1\nG0 X0" }]);
    await expectPath("/");
  });

  it("explains a failure and keeps the edits", async () => {
    const { user } = setup(recorder(500));
    await user.type(await screen.findByLabelText("Loaded gcode"), "\nM2");
    await user.click(screen.getByRole("button", { name: "Apply changes" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not apply the edited gcode.");
    expect(screen.getByLabelText("Loaded gcode")).toHaveValue("G21\nG0 X1 Y1\nG1 Z-1\nM2");
  });

  it("counts an empty file as no lines", async () => {
    setup(recorder(), "");
    expect(await screen.findByLabelText("Loaded gcode")).toHaveAccessibleDescription(/^0 lines/);
    expect(page().getByRole("link", { name: "Cancel" })).toHaveAttribute("href", "/files");
  });
});

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
