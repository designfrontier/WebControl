import { fireEvent, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeFetch, jsonResponse } from "../../test/fakeFetch";
import { topLayer } from "../../test/dialogPolyfill";
import { deferred, expectPath, recorder } from "../../test/filesBoards";
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
    expect(editor).toHaveAccessibleDescription(
      /^3 lines\. Line numbers match Go to line.* Applying replaces the loaded gcode/,
    );
    expect(screen.getByRole("button", { name: "Apply changes" })).toBeDisabled();
  });

  it("numbers the loaded program's lines beside the editor, hidden from assistive tech", async () => {
    const { user } = setup();
    const editor = await screen.findByLabelText("Loaded gcode");
    const gutter = screen.getByText("1 2 3");
    expect(gutter).toHaveAttribute("aria-hidden", "true");
    expect(editor).toHaveAccessibleDescription(/Line numbers match Go to line; blank lines in the file are dropped/);

    await user.type(editor, "\nM2");
    expect(gutter).toHaveTextContent("1 2 3 4");
  });

  it("keeps the line numbers level with the text while it scrolls", async () => {
    setup(recorder(), Array.from({ length: 200 }, (_, i) => `G0 X${i}`).join("\n"));
    const editor = await screen.findByLabelText("Loaded gcode");
    const gutter = screen.getByText(/^1 2 3 .* 200$/);
    editor.scrollTop = 840;
    fireEvent.scroll(editor);
    expect(gutter.scrollTop).toBe(840);
  });

  it("applies edits without trailing blank lines and returns to the control screen", async () => {
    const { user, edit } = setup();
    const editor = await screen.findByLabelText("Loaded gcode");
    await user.type(editor, "\nG0 X0{Enter}{Enter}");
    await user.click(screen.getByRole("button", { name: "Apply changes" }));

    expect(edit.forms()).toEqual([{ gcode: "G21\nG0 X1 Y1\nG1 Z-1\nG0 X0" }]);
    await expectPath("/");
  });

  it("covers the page with a loading overlay until the server has built the preview", async () => {
    const answer = deferred();
    const { user } = setup({ respond: answer.respond } as unknown as ReturnType<typeof recorder>);
    await user.type(await screen.findByLabelText("Loaded gcode"), "\nM2");
    await user.click(screen.getByRole("button", { name: "Apply changes" }));

    expect(screen.getByRole("button", { name: "Applying…" })).toBeDisabled();
    const overlay = screen.getByRole("dialog", { name: "Applying G-Code changes…" });
    expect(overlay).toHaveAccessibleDescription(/building its preview/);
    expect(topLayer().at(-1)).toBe(overlay);
    await user.keyboard("{Escape}");
    await expectPath("/files/edit");

    answer.settle.resolve(jsonResponse({ status: 200 }));
    await expectPath("/");
    expect(screen.queryByRole("dialog", { name: "Applying G-Code changes…" })).not.toBeInTheDocument();
  });

  it("clears the overlay when applying fails", async () => {
    const answer = deferred();
    const { user } = setup({ respond: answer.respond } as unknown as ReturnType<typeof recorder>);
    await user.type(await screen.findByLabelText("Loaded gcode"), "\nM2");
    await user.click(screen.getByRole("button", { name: "Apply changes" }));
    expect(screen.getByRole("dialog", { name: "Applying G-Code changes…" })).toBeInTheDocument();

    answer.settle.resolve(jsonResponse({ status: 409, error: "Stop the job first." }, 409));
    expect(await screen.findByRole("alert")).toHaveTextContent("Stop the job first.");
    expect(screen.queryByRole("dialog", { name: "Applying G-Code changes…" })).not.toBeInTheDocument();
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
