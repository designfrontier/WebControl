import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeFetch } from "../../test/fakeFetch";
import { expectPath, gcodeLibrary, pending, recorder } from "../../test/filesBoards";
import { renderApp } from "../../test/render";

const setup = (open = recorder(), library = gcodeLibrary()) => {
  const fetch = fakeFetch({ "/api/files/gcode": library, "POST /openGCode": open.respond });
  return { ...renderApp("/files/open", { fetch }), open };
};

describe("Open gcode", () => {
  it("starts in the last directory with the last file selected", async () => {
    setup();
    expect(await screen.findByRole("radio", { name: "leg.nc" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "arm.nc" })).not.toBeChecked();
    expect(screen.getByLabelText("Directory")).toHaveValue("jobs");
    expect(screen.queryByRole("radio", { name: "part.nc" })).not.toBeInTheDocument();
  });

  it("falls back to the top level when the last directory is gone", async () => {
    setup(recorder(), gcodeLibrary({ lastDirectory: "deleted", lastFile: "x.nc" }));
    expect(await screen.findByRole("radio", { name: "part.nc" })).not.toBeChecked();
    expect(screen.getByRole("option", { name: "Top level" })).toHaveProperty("selected", true);
  });

  it("lists another directory's files and clears the selection", async () => {
    const { user } = setup();
    await screen.findByRole("radio", { name: "leg.nc" });
    await user.selectOptions(screen.getByLabelText("Directory"), "Top level");
    expect(screen.getByRole("radio", { name: "part.nc" })).not.toBeChecked();

    await user.selectOptions(screen.getByLabelText("Directory"), "empty");
    expect(screen.getByText("No files in this directory.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Open" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Choose a file to open.");
  });

  it("opens the chosen file, asks for the new preview and returns to the control screen", async () => {
    const { user, open, socket } = setup();
    await user.click(await screen.findByRole("radio", { name: "arm.nc" }));
    await user.click(screen.getByRole("button", { name: "Open" }));

    expect(open.forms()).toEqual([{ selectedGCode: "jobs/arm.nc" }]);
    await expectPath("/");
    expect(socket.emittedEvents("checkForGCodeUpdate")).toEqual([{ data: "Please" }]);
  });

  it("uses ./name for files at the top level, as the classic route expects", async () => {
    const { user, open } = setup();
    await screen.findByRole("radio", { name: "leg.nc" });
    await user.selectOptions(screen.getByLabelText("Directory"), "Top level");
    await user.click(screen.getByRole("radio", { name: "part.nc" }));
    await user.click(screen.getByRole("button", { name: "Open" }));
    expect(open.forms()).toEqual([{ selectedGCode: "./part.nc" }]);
  });

  it("stays put and explains when the server cannot open it", async () => {
    const { user, socket } = setup(recorder(500));
    await user.click(await screen.findByRole("button", { name: "Open" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not open leg.nc.");
    await expectPath("/files/open");
    expect(socket.emittedEvents("checkForGCodeUpdate")).toEqual([]);
    expect(screen.getByRole("button", { name: "Open" })).toBeEnabled();
  });

  it("shows the server's reason when it refuses to open a file during a job", async () => {
    const error = "Cannot load gcode while a job is running or paused. Stop the job first.";
    const { user } = setup(recorder(409, { status: 409, error }));
    await user.click(await screen.findByRole("button", { name: "Open" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(error);
    await expectPath("/files/open");
  });

  it("shows progress while the server parses the file", async () => {
    const fetch = fakeFetch({ "/api/files/gcode": gcodeLibrary(), "POST /openGCode": pending });
    const { user } = renderApp("/files/open", { fetch });
    await user.click(await screen.findByRole("button", { name: "Open" }));
    expect(screen.getByRole("button", { name: "Opening…" })).toBeDisabled();
  });

  it("cancels back to the files screen", async () => {
    const { user } = setup();
    await user.click(await screen.findByRole("button", { name: "Cancel" }));
    await expectPath("/");
  });

  it("reports a library that cannot be listed", async () => {
    renderApp("/files/open");
    expect(screen.getByText("Loading the gcode library…")).toBeInTheDocument();
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not load the gcode library.");
  });
});
