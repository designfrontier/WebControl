import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeFetch, jsonResponse } from "../../test/fakeFetch";
import { renderApp } from "../../test/render";
import { libraryTool } from "./fixtures";
import { toolServer } from "../../test/tools";

const endmill = libraryTool(1, '1/4" 2 flute endmill', { diameter: 6.35, units: "mm", flutes: 2, notes: "Upcut" });

describe("Settings ▸ Tools", () => {
  it("lists the library with each tool's size and notes", async () => {
    const { fetch } = toolServer([endmill, libraryTool(3, "60° v-bit")]);
    renderApp("/settings/tools", { fetch });
    const list = await screen.findByRole("list", { name: "Tool library" });
    const rows = within(list).getAllByRole("listitem");
    expect(rows.map((row) => row.textContent)).toEqual([
      expect.stringContaining('Tool 1: 1/4" 2 flute endmill6.35 mm · 2 flutesUpcut'),
      expect.stringContaining("Tool 3: 60° v-bit"),
    ]);
    expect(screen.getByRole("link", { name: "Tools" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("link", { name: "How the tool library works" })).toHaveAttribute(
      "href",
      "/help/GettingStarted/toolLibrary.md",
    );
  });

  it("adds a tool numbered after the last, and checks it before sending", async () => {
    const { fetch, requests } = toolServer([endmill]);
    const { user } = renderApp("/settings/tools", { fetch });
    await user.click(await screen.findByRole("button", { name: "Add tool" }));
    const form = screen.getByRole("form", { name: "New tool" });
    expect(within(form).getByLabelText("Tool number")).toHaveValue(2);
    await user.click(within(form).getByRole("button", { name: "Add tool" }));
    expect(within(form).getByRole("alert")).toHaveTextContent("Describe the tool");
    expect(requests()).toEqual([]);

    await user.type(within(form).getByLabelText("Description"), '1/8" ball');
    await user.type(within(form).getByLabelText("Diameter"), "0.125");
    await user.selectOptions(within(form).getByLabelText("Diameter units"), "in");
    await user.type(within(form).getByLabelText("Flutes"), "2");
    await user.click(within(form).getByRole("button", { name: "Add tool" }));
    expect(await screen.findByText('1/8" ball')).toBeInTheDocument();
    expect(screen.getByText("0.125 in · 2 flutes")).toBeInTheDocument();
    expect(requests()).toEqual([
      {
        method: "POST",
        path: "/api/tools",
        body: { number: 2, description: '1/8" ball', diameter: 0.125, units: "in", flutes: 2, notes: "" },
      },
    ]);
    expect(screen.queryByRole("form")).not.toBeInTheDocument();
  });

  it("edits a tool in place, and shows the server's refusal", async () => {
    const { fetch, requests } = toolServer([endmill, libraryTool(3, "v-bit")]);
    const { user } = renderApp("/settings/tools", { fetch });
    await user.click(await screen.findByRole("button", { name: "Edit tool 1" }));
    const form = screen.getByRole("form", { name: "Edit tool 1" });
    expect(within(form).getByLabelText("Notes")).toHaveValue("Upcut");
    await user.clear(within(form).getByLabelText("Tool number"));
    await user.type(within(form).getByLabelText("Tool number"), "3");
    await user.click(within(form).getByRole("button", { name: "Save tool" }));
    expect(await within(form).findByRole("alert")).toHaveTextContent("Your tool library already has a tool 3.");

    await user.clear(within(form).getByLabelText("Tool number"));
    await user.type(within(form).getByLabelText("Tool number"), "2");
    await user.clear(within(form).getByLabelText("Description"));
    await user.type(within(form).getByLabelText("Description"), "6 mm endmill");
    await user.click(within(form).getByRole("button", { name: "Save tool" }));
    expect(await screen.findByText("6 mm endmill")).toBeInTheDocument();
    expect(requests().at(-1)).toMatchObject({ method: "PUT", path: "/api/tools/1", body: { number: 2 } });

    await user.click(screen.getByRole("button", { name: "Edit tool 2" }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("form")).not.toBeInTheDocument();
  });

  it("deletes a tool only after a second press", async () => {
    const { fetch, requests } = toolServer([endmill]);
    const { user } = renderApp("/settings/tools", { fetch });
    await user.click(await screen.findByRole("button", { name: "Delete tool 1" }));
    await user.click(screen.getByRole("button", { name: "Keep" }));
    expect(requests()).toEqual([]);
    await user.click(screen.getByRole("button", { name: "Delete tool 1" }));
    await user.click(screen.getByRole("button", { name: "Delete tool 1" }));
    expect(await screen.findByText(/Your tool library is empty/)).toBeInTheDocument();
    expect(requests()).toEqual([{ method: "DELETE", path: "/api/tools/1", body: undefined }]);
  });

  it("says when a delete fails", async () => {
    const fetch = fakeFetch({
      "/api/tools": { tools: [endmill] },
      "DELETE /api/tools/1": () => jsonResponse({ nope: true }, 500),
    });
    const { user } = renderApp("/settings/tools", { fetch });
    await user.click(await screen.findByRole("button", { name: "Delete tool 1" }));
    await user.click(screen.getByRole("button", { name: "Delete tool 1" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not delete tool 1. Try again.");
  });

  it("follows other clients' edits", async () => {
    const { fetch } = toolServer([]);
    const { socket } = renderApp("/settings/tools", { fetch });
    expect(await screen.findByText(/Your tool library is empty/)).toBeInTheDocument();
    socket.serverMessage("toolLibrary", { tools: [endmill] });
    expect(await screen.findByText('1/4" 2 flute endmill')).toBeInTheDocument();
  });

  it("offers to try again when the library cannot be loaded", async () => {
    let fail = true;
    const fetch = fakeFetch({
      "/api/tools": () => (fail ? jsonResponse({ error: "broken" }, 500) : jsonResponse({ tools: [endmill] })),
    });
    const { user } = renderApp("/settings/tools", { fetch });
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not load the tool library.");
    fail = false;
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText('1/4" 2 flute endmill')).toBeInTheDocument();
  });
});
