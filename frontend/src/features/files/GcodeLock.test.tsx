import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeFetch, jsonResponse, startupState } from "../../test/fakeFetch";
import { gcodeLibrary } from "../../test/filesBoards";
import { renderApp } from "../../test/render";

const lockedApp = (path: string) =>
  renderApp(path, {
    fetch: fakeFetch({
      "GET /api/state": startupState({ jobInProgress: true }),
      "/api/files/gcode": gcodeLibrary(),
      "/api/files/gcode/text": { text: "G21" },
      "/api/files/gcode/custom": { text: "G0 X1" },
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

  it("refuses to send custom gcode, saying why", async () => {
    lockedApp("/files/send");
    expect(await screen.findByText(/Stop the job first to send gcode/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Send" })).toBeDisabled();
  });

  it.each([
    ["/files/import/restore", "Restore"],
    ["/files/import/groundcontrol", "Import"],
    ["/files/import/webcontrol", "Import"],
  ])("%s refuses to change the settings, saying why", async (path, submit) => {
    lockedApp(path);
    expect(await screen.findByText(/Stop the job first to change the settings/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: submit })).toBeDisabled();
  });
});

describe("a job that starts after the screen opened", () => {
  it("shows the Rust backend's refusal of a send", async () => {
    const fetch = fakeFetch({
      "/api/files/gcode/custom": { text: "G0 X1" },
      "POST /sendGCode": () =>
        jsonResponse(
          { status: 409, error: "Cannot send gcode while a job is running or paused. Stop the job first." },
          409,
        ),
    });
    const { user } = renderApp("/files/send", { fetch });
    await user.click(await screen.findByRole("button", { name: "Send" }));
    expect(
      await screen.findByText("Cannot send gcode while a job is running or paused. Stop the job first."),
    ).toBeInTheDocument();
  });

  it("shows the Rust backend's refusal of an import", async () => {
    const fetch = fakeFetch({
      "POST /importFile": () =>
        jsonResponse(
          { status: 409, error: "Cannot change settings while a job is running or paused. Stop the job first." },
          409,
        ),
    });
    const { user } = renderApp("/files/import/groundcontrol", { fetch });
    await user.upload(await screen.findByLabelText("File"), new File(["x"], "groundcontrol.ini"));
    await user.click(screen.getByRole("button", { name: "Import" }));
    expect(
      await screen.findByText("Cannot change settings while a job is running or paused. Stop the job first."),
    ).toBeInTheDocument();
  });
});
