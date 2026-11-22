import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { Tool } from "../../protocol";
import { renderApp } from "../../test/render";
import { toolServer } from "../../test/tools";
import { libraryTool } from "./fixtures";

const endmill = libraryTool(1, '1/4" 2 flute endmill', { diameter: 6.35, units: "mm", flutes: 2 });

const modal = (tool?: number, message = `Please insert tool ${tool ?? 2}`) => ({
  title: "Notification:",
  message,
  resume: "resume",
  progress: "false",
  modalSize: "small",
  modalType: "notification",
  ...(tool === undefined ? {} : { tool }),
});

const prompt = (fetch: ReturnType<typeof toolServer>["fetch"], tool?: number, message?: string) => {
  const app = renderApp("/", { fetch });
  app.socket.serverMessage("activateModal", modal(tool, message));
  return { ...app, dialog: screen.getByRole("dialog", { name: "Notification" }) };
};

const library = (tools: Tool[]) => toolServer(tools).fetch;

describe("the tool-change prompt", () => {
  it("folds the firmware's insert-tool line into the tool's name from the library", async () => {
    const { dialog } = prompt(library([endmill]), 1);
    expect(await within(dialog).findByText('Insert Tool 1 — 1/4" 2 flute endmill')).toBeInTheDocument();
    expect(within(dialog).queryByText(/Please insert/)).not.toBeInTheDocument();
    expect(dialog).toHaveAccessibleDescription('Insert Tool 1 — 1/4" 2 flute endmill');
    expect(within(dialog).getByRole("button", { name: "Resume" })).toBeInTheDocument();
  });

  it("asks for the tool by number while the library loads", () => {
    const { dialog } = prompt(library([endmill]), 1);
    expect(within(dialog).getByText("Insert Tool 1")).toBeInTheDocument();
  });

  it("says when the library does not know the tool", async () => {
    const { dialog } = prompt(library([endmill]), 3);
    expect(await within(dialog).findByText("Insert Tool 3 (not in your tool library)")).toBeInTheDocument();
  });

  it("folds a prompt for a tool without a number into the one WebControl selected", async () => {
    const { dialog } = prompt(library([endmill]), 1, "Please insert a tool");
    expect(await within(dialog).findByText('Insert Tool 1 — 1/4" 2 flute endmill')).toBeInTheDocument();
  });

  it("keeps any other firmware text above the tool's name", async () => {
    const { dialog } = prompt(library([endmill]), 1, "Please insert tool 1 and zero Z");
    expect(within(dialog).getByText("Please insert tool 1 and zero Z")).toBeInTheDocument();
    expect(await within(dialog).findByText('Tool 1 — 1/4" 2 flute endmill')).toBeInTheDocument();
    expect(dialog).toHaveAccessibleDescription("Please insert tool 1 and zero Z");
  });

  it("keeps the firmware's line when it asks for another number", async () => {
    const { dialog } = prompt(library([endmill]), 1, "Please insert tool 5");
    expect(within(dialog).getByText("Please insert tool 5")).toBeInTheDocument();
    expect(await within(dialog).findByText('Tool 1 — 1/4" 2 flute endmill')).toBeInTheDocument();
  });

  it("counts a repeated prompt on the folded line", async () => {
    const { dialog, socket } = prompt(library([endmill]), 1);
    socket.serverMessage("activateModal", modal(1));
    expect(await within(dialog).findByText("(×2)")).toBeInTheDocument();
    expect(within(dialog).getByText(/^Insert Tool 1/)).toHaveTextContent('Insert Tool 1 — 1/4" 2 flute endmill (×2)');
  });

  it("follows a library edit made while it waits", async () => {
    const { dialog, socket } = prompt(library([]), 1);
    expect(await within(dialog).findByText("Insert Tool 1 (not in your tool library)")).toBeInTheDocument();
    socket.serverMessage("toolLibrary", { tools: [endmill] });
    expect(await within(dialog).findByText('Insert Tool 1 — 1/4" 2 flute endmill')).toBeInTheDocument();
  });

  it("shows the firmware's words alone when no tool is named", () => {
    const { dialog } = prompt(library([endmill]), undefined, "Please insert a tool");
    expect(within(dialog).getByText("Please insert a tool")).toBeInTheDocument();
    expect(within(dialog).queryByText(/Tool /)).not.toBeInTheDocument();
  });

  it("names the tool by number alone when the library cannot be read", async () => {
    const broken = toolServer([], { "/api/tools": () => new Response("broken", { status: 500 }) });
    const { dialog } = prompt(broken.fetch, 2);
    expect(await within(dialog).findByText("Insert Tool 2 (not in your tool library)")).toBeInTheDocument();
  });
});
