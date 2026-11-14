import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeFetch, jsonResponse } from "../../test/fakeFetch";
import { gcodeLibrary, recorder } from "../../test/filesBoards";
import { currentPath, renderApp } from "../../test/render";

const setup = (save = recorder()) => {
  const fetch = fakeFetch({
    "/api/files/gcode": gcodeLibrary(),
    "POST /saveGCode": save.respond,
    "POST /api/files/gcode/directories": () =>
      jsonResponse({ directory: "new", directories: [".", "empty", "jobs", "new"], files: gcodeLibrary().files }, 201),
  });
  return { ...renderApp("/files/save", { fetch }), save };
};

const nameField = () => screen.getByLabelText("File name");

describe("Save gcode as", () => {
  it("offers the last file name and warns that it will be replaced", async () => {
    setup();
    expect(await screen.findByLabelText("File name")).toHaveValue("leg.nc");
    expect(screen.getByRole("radio", { name: "leg.nc" })).toBeChecked();
    expect(screen.getByText("Saving replaces the existing leg.nc.")).toHaveClass("warning");
  });

  it("fills the name in from an existing file", async () => {
    const { user } = setup();
    await user.click(await screen.findByRole("radio", { name: "arm.nc" }));
    expect(nameField()).toHaveValue("arm.nc");
  });

  it("saves a new name in the chosen directory and returns to the files screen", async () => {
    const { user, save } = setup();
    await user.clear(await screen.findByLabelText("File name"));
    await user.type(nameField(), " leg-v2.nc ");
    expect(screen.getByText("Pick an existing file above to replace it.")).toBeInTheDocument();
    expect(screen.queryByRole("radio", { checked: true })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(save.forms()).toEqual([{ selectedDirectory: "jobs", fileName: "leg-v2.nc" }]);
    expect(currentPath()).toBe("/files");
  });

  it.each([
    ["", "Enter a file name."],
    ["jobs/leg.nc", "A file name cannot contain slashes."],
  ])("rejects the name %j", async (name, message) => {
    const { user, save } = setup();
    await user.clear(await screen.findByLabelText("File name"));
    if (name) await user.type(nameField(), name);
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(screen.getByRole("alert")).toHaveTextContent(message);
    expect(save.calls).toEqual([]);
  });

  it("explains a failed save", async () => {
    const { user } = setup(recorder(500));
    await user.click(await screen.findByRole("button", { name: "Save" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not save leg.nc.");
  });

  it("saves into a directory it just created", async () => {
    const { user, save } = setup();
    await user.type(await screen.findByLabelText("New directory"), "new");
    await user.click(screen.getByRole("button", { name: "Create directory" }));
    expect(await screen.findByRole("option", { name: "new" })).toHaveProperty("selected", true);
    expect(screen.getByText("No files in this directory.")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(save.forms()).toEqual([{ selectedDirectory: "new", fileName: "leg.nc" }]);
  });
});
