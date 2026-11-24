import { describe, expect, it } from "vitest";
import { gcodeLibrary } from "../../test/filesBoards";
import {
  deletePrompt,
  describeLocation,
  directoryLabel,
  fileNameProblem,
  filesIn,
  initialDirectory,
  initialFile,
  libraryPath,
} from "./library";

describe("library helpers", () => {
  it("filters files by directory", () => {
    expect(filesIn(gcodeLibrary(), "jobs").map((file) => file.name)).toEqual(["arm.nc", "leg.nc"]);
    expect(filesIn(gcodeLibrary(), "empty")).toEqual([]);
  });

  it("builds the directory/name path the classic routes take", () => {
    expect(libraryPath({ directory: ".", name: "a.nc" })).toBe("./a.nc");
    expect(libraryPath({ directory: "jobs", name: "a.nc" })).toBe("jobs/a.nc");
  });

  it("labels the top level in words", () => {
    expect(directoryLabel(".")).toBe("Top level");
    expect(directoryLabel("jobs")).toBe("jobs");
  });

  it("starts from the last directory and file only when they still exist", () => {
    expect(initialDirectory(gcodeLibrary())).toBe("jobs");
    expect(initialDirectory(gcodeLibrary({ lastDirectory: "gone" }))).toBe(".");
    expect(initialFile(gcodeLibrary(), "jobs")).toBe("leg.nc");
    expect(initialFile(gcodeLibrary(), ".")).toBeNull();
  });

  it.each([
    ["part.nc", null],
    ["", "Enter a file name."],
    ["   ", "Enter a file name."],
    ["a/b.nc", "A file name cannot contain slashes."],
    ["a\\b.nc", "A file name cannot contain slashes."],
  ])("checks the file name %j", (name, problem) => {
    expect(fileNameProblem(name)).toBe(problem);
  });

  it("describes a location without the top-level prefix", () => {
    expect(describeLocation(".", "a.nc")).toBe("a.nc");
    expect(describeLocation(null, "a.nc")).toBe("a.nc");
    expect(describeLocation("jobs", "a.nc")).toBe("jobs/a.nc");
  });

  it.each([
    [undefined, "Delete jobs? This cannot be undone."],
    [0, "Delete the empty directory jobs? This cannot be undone."],
    [1, "Delete the directory jobs and the file in it? This cannot be undone."],
    [3, "Delete the directory jobs and the 3 files in it? This cannot be undone."],
  ])("asks before deleting with %j files", (files, prompt) => {
    expect(deletePrompt({ path: "jobs", label: "jobs", files })).toBe(prompt);
  });
});
