import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeFetch } from "../../test/fakeFetch";
import { expectPath, pending, recorder } from "../../test/filesBoards";
import { renderApp } from "../../test/render";

const page = () => within(screen.getByRole("dialog", { name: /G-Code/ }));

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

  it("shows it is busy until the server has built the preview", async () => {
    const fetch = fakeFetch({ "/api/files/gcode/text": { text: "G0 X1" }, "POST /editGCode": pending });
    const { user } = renderApp("/files/edit", { fetch });
    await user.type(await screen.findByLabelText("Loaded gcode"), "\nM2");
    await user.click(screen.getByRole("button", { name: "Apply changes" }));

    expect(screen.getByRole("button", { name: "Applying…" })).toBeDisabled();
    expect(screen.getByText(/building its preview/)).toHaveAttribute("role", "status");
    await expectPath("/files/edit");
  });

  it("explains a failure and keeps the edits", async () => {
    const { user } = setup(recorder(500));
    await user.type(await screen.findByLabelText("Loaded gcode"), "\nM2");
    await user.click(screen.getByRole("button", { name: "Apply changes" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not apply the edited gcode.");
    expect(screen.getByLabelText("Loaded gcode")).toHaveValue("G21\nG0 X1 Y1\nG1 Z-1\nM2");
  });

  it("shows the server's reason when it refuses the edit during a job", async () => {
    const error = "Cannot load gcode while a job is running or paused. Stop the job first.";
    const { user } = setup(recorder(409, { status: 409, error }));
    await user.type(await screen.findByLabelText("Loaded gcode"), "\nM2");
    await user.click(screen.getByRole("button", { name: "Apply changes" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(error);
  });

  it("counts an empty file as no lines", async () => {
    setup(recorder(), "");
    expect(await screen.findByLabelText("Loaded gcode")).toHaveAccessibleDescription(/^0 lines/);
    expect(page().getByRole("button", { name: "Cancel" })).toBeInTheDocument();
  });
});
