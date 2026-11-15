import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeFetch, startupState } from "../../test/fakeFetch";
import { gcodeLibrary } from "../../test/filesBoards";
import { renderApp } from "../../test/render";

const lockedApp = (path: string) =>
  renderApp(path, {
    fetch: fakeFetch({
      "GET /api/state": startupState({ jobInProgress: true }),
      "/api/files/gcode": gcodeLibrary(),
      "/api/files/gcode/text": { text: "G21" },
    }),
  });

describe("gcode modals during a job", () => {
  it.each([
    ["/files/open", "Open"],
    ["/files/upload", "Upload and open"],
  ])("%s refuses to load other gcode, saying why", async (path, submit) => {
    lockedApp(path);
    expect(await screen.findByText(/Stop the job first to load other gcode/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: submit })).toBeDisabled();
  });

  it("refuses to apply edits, saying why", async () => {
    const { user } = lockedApp("/files/edit");
    await user.type(await screen.findByLabelText("Loaded gcode"), "\nM2");
    expect(await screen.findByText(/Stop the job first to apply changes/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Apply changes" })).toBeDisabled();
  });
});
