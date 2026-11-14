import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeFetch, jsonResponse } from "../../test/fakeFetch";
import { boardLibrary, expectPath, recorder } from "../../test/filesBoards";
import { renderApp } from "../../test/render";

describe("Open board", () => {
  const setup = (open = recorder()) => ({
    ...renderApp("/boards/open", {
      fetch: fakeFetch({ "/api/boards/files": boardLibrary(), "POST /openBoard": open.respond }),
    }),
    open,
  });

  it("opens the chosen board and returns to the control screen", async () => {
    const { user, open } = setup();
    expect(await screen.findByRole("radio", { name: "shelf.brd" })).toBeChecked();
    await user.selectOptions(screen.getByLabelText("Directory"), "shop");
    await user.click(screen.getByRole("radio", { name: "bench.brd" }));
    await user.click(screen.getByRole("button", { name: "Open" }));

    expect(open.forms()).toEqual([{ selectedBoard: "shop/bench.brd" }]);
    await expectPath("/");
  });

  it("explains a board that cannot be opened", async () => {
    const { user } = setup(recorder(500));
    await user.click(await screen.findByRole("button", { name: "Open" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not open shelf.brd.");
    await user.click(screen.getByRole("link", { name: "Cancel" }));
    await expectPath("/boards");
  });

  it("reports a library that cannot be listed", async () => {
    renderApp("/boards/open");
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not load the board library.");
  });
});

describe("Save board as", () => {
  const setup = (save = recorder()) => ({
    ...renderApp("/boards/save", {
      fetch: fakeFetch({
        "/api/boards/files": boardLibrary(),
        "POST /saveBoard": save.respond,
        "POST /api/boards/directories": () =>
          jsonResponse({ directory: "garage", directories: [".", "garage", "shop"], files: [] }, 201),
      }),
    }),
    save,
  });

  it("saves under a new name and returns to the boards screen", async () => {
    const { user, save } = setup();
    await user.clear(await screen.findByLabelText("File name"));
    await user.type(screen.getByLabelText("File name"), "shelf-2.brd");
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(save.forms()).toEqual([{ selectedDirectory: ".", fileName: "shelf-2.brd" }]);
    await expectPath("/boards");
  });

  it("creates a board directory to save into", async () => {
    const { user, save } = setup();
    await user.type(await screen.findByLabelText("New directory"), "garage");
    await user.click(screen.getByRole("button", { name: "Create directory" }));
    expect(await screen.findByRole("option", { name: "garage" })).toHaveProperty("selected", true);

    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(save.forms()).toEqual([{ selectedDirectory: "garage", fileName: "shelf.brd" }]);
  });
});
