import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeFetch } from "../../test/fakeFetch";
import { boardDetails, expectPath } from "../../test/filesBoards";
import { renderApp } from "../../test/render";

const details = () => within(screen.getByRole("region", { name: "Current board" }));
const detail = (term: string) => details().getByText(term).nextElementSibling;

const setup = (board = boardDetails()) =>
  renderApp("/boards", { fetch: fakeFetch({ "/api/boards/current": () => Response.json(board) }) });

describe("Boards home", () => {
  it("describes the current board in the machine's units", async () => {
    setup(boardDetails({ units: "mm", width: 1219.2, height: 609.6, thickness: 19.05, centerX: -10, centerY: 2.5 }));
    expect(screen.getByRole("heading", { name: "Boards", level: 1 })).toBeInTheDocument();
    expect(await details().findByText("Shelf")).toBeInTheDocument();
    expect(detail("Material")).toHaveTextContent("Plywood");
    expect(detail("Width")).toHaveTextContent("1219.2 mm");
    expect(detail("Thickness")).toHaveTextContent("19.05 mm");
    expect(detail("Center")).toHaveTextContent("X -10 mm, Y 2.5 mm");
    expect(detail("File")).toHaveTextContent("shelf.brd");
  });

  it("uses inches and says when the board has not been saved", async () => {
    setup(boardDetails({ fileName: "" }));
    expect(await details().findByText("48 in")).toBeInTheDocument();
    expect(detail("File")).toHaveTextContent("Not saved");
  });

  it("refreshes when the server pushes a board update", async () => {
    let board = boardDetails();
    const fetch = fakeFetch({ "/api/boards/current": () => Response.json(board) });
    const { socket } = renderApp("/boards", { fetch });
    await details().findByText("Shelf");

    board = boardDetails({ boardID: "Bench" });
    socket.serverMessage("boardDataUpdate", { ...board, fileName: "" });
    expect(await details().findByText("Bench")).toBeInTheDocument();
  });

  it("marks the loaded gcode as cut", async () => {
    const { user, socket } = setup();
    await user.click(screen.getByRole("button", { name: "Mark loaded gcode as cut" }));
    expect(socket.actions()).toEqual([["boardProcessGCode", "", ""]]);
  });

  it("asks before clearing the cut record", async () => {
    const { user, socket } = setup();
    await user.click(screen.getByRole("button", { name: "Clear cut data…" }));
    const confirm = within(screen.getByRole("group", { name: "Forget which parts of this board have been cut?" }));
    await user.click(confirm.getByRole("button", { name: "Keep it" }));
    expect(socket.actions()).toEqual([]);

    await user.click(screen.getByRole("button", { name: "Clear cut data…" }));
    await user.click(screen.getByRole("button", { name: "Clear cut data" }));
    expect(socket.actions()).toEqual([["boardClearBoard", "", ""]]);
    expect(screen.getByRole("button", { name: "Clear cut data…" })).toBeInTheDocument();
  });

  it("reports a board that cannot be loaded", async () => {
    renderApp("/boards");
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not load the current board.");
  });

  it.each([
    ["Open…", "/boards/open", "Open board"],
    ["Save as…", "/boards/save", "Save board as"],
    ["Create or edit…", "/boards/edit", "Create or edit board"],
    ["Trim…", "/boards/trim", "Trim board"],
  ])("%s opens its own screen", async (link, path, heading) => {
    const { user } = setup();
    await user.click(screen.getByRole("link", { name: link }));
    await expectPath(path);
    expect(screen.getByRole("heading", { name: heading, level: 2 })).toBeInTheDocument();
    await user.click(screen.getByRole("link", { name: "Back" }));
    await expectPath("/boards");
  });

  it("sends unknown sub-paths back to the boards screen", async () => {
    renderApp("/boards/nope");
    await expectPath("/boards");
  });
});
