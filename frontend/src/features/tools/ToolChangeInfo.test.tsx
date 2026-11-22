import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { Tool } from "../../protocol";
import { renderApp } from "../../test/render";
import { toolServer } from "../../test/tools";
import { libraryTool } from "./fixtures";

const endmill = libraryTool(1, '1/4" 2 flute endmill', { diameter: 6.35, units: "mm", flutes: 2 });

const prompt = (fetch: ReturnType<typeof toolServer>["fetch"], tool?: number) => {
  const app = renderApp("/", { fetch });
  app.socket.serverMessage("activateModal", {
    title: "Notification:",
    message: `Please insert tool ${tool ?? "a tool"}`,
    resume: "resume",
    progress: "false",
    modalSize: "small",
    modalType: "notification",
    ...(tool === undefined ? {} : { tool }),
  });
  return { ...app, dialog: screen.getByRole("dialog", { name: "Notification" }) };
};

const library = (tools: Tool[]) => toolServer(tools).fetch;

describe("the tool-change prompt", () => {
  it("names a tool from the library", async () => {
    const { dialog } = prompt(library([endmill]), 1);
    expect(within(dialog).getByText("Please insert tool 1")).toBeInTheDocument();
    expect(await within(dialog).findByText('Tool 1 — 1/4" 2 flute endmill')).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Resume" })).toBeInTheDocument();
  });

  it("says when the library does not know the tool", async () => {
    const { dialog } = prompt(library([endmill]), 3);
    expect(await within(dialog).findByText("Tool 3 (not in your tool library)")).toBeInTheDocument();
  });

  it("follows a library edit made while it waits", async () => {
    const { dialog, socket } = prompt(library([]), 1);
    expect(await within(dialog).findByText("Tool 1 (not in your tool library)")).toBeInTheDocument();
    socket.serverMessage("toolLibrary", { tools: [endmill] });
    expect(await within(dialog).findByText('Tool 1 — 1/4" 2 flute endmill')).toBeInTheDocument();
  });

  it("says nothing more when the firmware names no tool", () => {
    const { dialog } = prompt(library([endmill]));
    expect(within(dialog).getByText("Please insert tool a tool")).toBeInTheDocument();
    expect(within(dialog).queryByText(/^Tool /)).not.toBeInTheDocument();
  });

  it("names the tool by number alone when the library cannot be read", async () => {
    const broken = toolServer([], { "/api/tools": () => new Response("broken", { status: 500 }) });
    const { dialog } = prompt(broken.fetch, 2);
    expect(await within(dialog).findByText("Tool 2 (not in your tool library)")).toBeInTheDocument();
  });
});
