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
