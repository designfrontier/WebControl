import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { FileTool, Tool } from "../../protocol";
import { startupState } from "../../test/fakeFetch";
import { renderApp } from "../../test/render";
import { toolServer } from "../../test/tools";
import { fileTool, libraryTool } from "./fixtures";

const endmill = libraryTool(1, '1/4" 2 flute endmill', { diameter: 6.35, units: "mm", flutes: 2 });

const withFile = (library: Tool[], fileTools: FileTool[]) =>
  toolServer(library, fileTools, { "GET /api/state": startupState({ gcodeFile: "jobs/part.nc" }) });

const modal = (tool?: number, message = `Please insert tool ${tool ?? 2}`) => ({
  title: "Notification:",
  message,
  resume: "resume",
  progress: "false",
  modalSize: "small",
  modalType: "notification",
  ...(tool === undefined ? {} : { tool }),
});

const prompt = (server: ReturnType<typeof withFile>, tool?: number, message?: string) => {
  const app = renderApp("/", { fetch: server.fetch });
  app.socket.serverMessage("activateModal", modal(tool, message));
  return { ...app, dialog: screen.getByRole("dialog", { name: "Notification" }) };
};

describe("the tool-change prompt", () => {
  it("names a library tool the file does not describe", async () => {
    const { dialog } = prompt(withFile([endmill], [fileTool(1, null)]), 1);
    expect(await within(dialog).findByText('Insert Tool 1 — 1/4" 2 flute endmill')).toBeInTheDocument();
    expect(within(dialog).queryByText(/Please insert/)).not.toBeInTheDocument();
    expect(dialog).toHaveAccessibleDescription('Insert Tool 1 — 1/4" 2 flute endmill');
    expect(within(dialog).queryByRole("button", { name: /tool library/ })).not.toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Resume" })).toBeInTheDocument();
  });

  it("says when neither the file nor the library knows the tool", async () => {
    const { dialog } = prompt(withFile([endmill], []), 3);
    expect(await within(dialog).findByText("Insert Tool 3 (not in your tool library)")).toBeInTheDocument();
    expect(within(dialog).queryByRole("button", { name: /tool library/ })).not.toBeInTheDocument();
  });

  it("shows the file's description and adds it to the library, edited first, in one click", async () => {
    const server = withFile([endmill], [fileTool(6, '6.35 mm (1/4") flat end mill', { diameter: 6.35, units: "mm" })]);
    const { dialog, user } = prompt(server, 6);
    expect(await within(dialog).findByText('Insert Tool 6 — 6.35 mm (1/4") flat end mill')).toBeInTheDocument();
    expect(within(dialog).getByText("From this file; not in your tool library.")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Add to tool library" }));
    const form = within(dialog).getByRole("form", { name: "New tool" });
    expect(within(form).getByLabelText("Description")).toHaveValue('6.35 mm (1/4") flat end mill');
    expect(within(form).getByLabelText("Diameter")).toHaveValue(6.35);
    await user.type(within(form).getByLabelText("Flutes"), "2");
    await user.click(within(form).getByRole("button", { name: "Add to tool library" }));
    expect(await within(dialog).findByText('Insert Tool 6 — 6.35 mm (1/4") flat end mill')).toBeInTheDocument();
    expect(within(dialog).queryByText(/From this file/)).not.toBeInTheDocument();
    expect(server.requests()).toEqual([
      {
        method: "POST",
        path: "/api/tools",
        body: {
          number: 6,
          description: '6.35 mm (1/4") flat end mill',
          diameter: 6.35,
          units: "mm",
          flutes: 2,
          notes: "",
        },
      },
    ]);
  });

  it("shows the file's description over a different library one and offers to update the entry", async () => {
    const server = withFile([endmill], [fileTool(1, "6 mm 2 flute flat end mill", { diameter: 6, units: "mm" })]);
    const { dialog, user } = prompt(server, 1);
    expect(await within(dialog).findByText("Insert Tool 1 — 6 mm 2 flute flat end mill")).toBeInTheDocument();
    expect(within(dialog).getByText('From this file; your library says 1/4" 2 flute endmill.')).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Update tool library" }));
    const form = within(dialog).getByRole("form", { name: "Edit tool 1" });
    await user.click(within(form).getByRole("button", { name: "Cancel" }));
    expect(server.requests()).toEqual([]);
    await user.click(within(dialog).getByRole("button", { name: "Update tool library" }));
    await user.click(within(dialog).getByRole("button", { name: "Update tool library" }));
    expect(await within(dialog).findByText("Insert Tool 1 — 6 mm 2 flute flat end mill")).toBeInTheDocument();
    expect(within(dialog).queryByText(/your library says/)).not.toBeInTheDocument();
    expect(server.library()[0]).toMatchObject({
      number: 1,
      description: "6 mm 2 flute flat end mill",
      diameter: 6,
      flutes: 2,
    });
  });

  it("asks for the tool by number while the library loads", () => {
    const { dialog } = prompt(withFile([endmill], []), 1);
    expect(within(dialog).getByText("Insert Tool 1")).toBeInTheDocument();
  });

  it("folds a prompt for a tool without a number into the one WebControl selected", async () => {
    const { dialog } = prompt(withFile([endmill], []), 1, "Please insert a tool");
    expect(await within(dialog).findByText('Insert Tool 1 — 1/4" 2 flute endmill')).toBeInTheDocument();
  });

  it("keeps any other firmware text above the tool's name", async () => {
    const { dialog } = prompt(withFile([endmill], []), 1, "Please insert tool 1 and zero Z");
    expect(within(dialog).getByText("Please insert tool 1 and zero Z")).toBeInTheDocument();
    expect(await within(dialog).findByText('Tool 1 — 1/4" 2 flute endmill')).toBeInTheDocument();
    expect(dialog).toHaveAccessibleDescription("Please insert tool 1 and zero Z");
  });

  it("keeps the firmware's line when it asks for another number", async () => {
    const { dialog } = prompt(withFile([endmill], []), 1, "Please insert tool 5");
    expect(within(dialog).getByText("Please insert tool 5")).toBeInTheDocument();
    expect(await within(dialog).findByText('Tool 1 — 1/4" 2 flute endmill')).toBeInTheDocument();
  });

  it("counts a repeated prompt on the folded line", async () => {
    const { dialog, socket } = prompt(withFile([endmill], []), 1);
    await within(dialog).findByText('Insert Tool 1 — 1/4" 2 flute endmill');
    socket.serverMessage("activateModal", modal(1));
    expect(await within(dialog).findByText("(×2)")).toBeInTheDocument();
    expect(within(dialog).getByText(/^Insert Tool 1/)).toHaveTextContent('Insert Tool 1 — 1/4" 2 flute endmill (×2)');
  });

  it("shows the firmware's words alone when no tool is named", () => {
    const { dialog } = prompt(withFile([endmill], []), undefined, "Please insert a tool");
    expect(within(dialog).getByText("Please insert a tool")).toBeInTheDocument();
    expect(within(dialog).queryByText(/Tool /)).not.toBeInTheDocument();
  });

  it("names the tool from the file alone when the library cannot be read", async () => {
    const server = toolServer([], [fileTool(2, "ball end mill")], {
      "GET /api/state": startupState({ gcodeFile: "part.nc" }),
      "/api/tools": () => new Response("broken", { status: 500 }),
    });
    const { dialog } = prompt(server, 2);
    expect(await within(dialog).findByText("Insert Tool 2 — ball end mill")).toBeInTheDocument();
  });
});

describe("the loaded file's tools", () => {
  it("lists them folded under the file name, with what needs review", async () => {
    const server = withFile(
      [endmill, libraryTool(5, "1/8 endmill")],
      [fileTool(5, null), fileTool(6, "6.35 mm flat end mill", { diameter: 6.35, units: "mm" }), fileTool(9, null)],
    );
    const { user } = renderApp("/", { fetch: server.fetch });
    const summary = await screen.findByText(/Tools: T5, T6, T9/);
    expect(await screen.findByText("(1 to review)")).toBeInTheDocument();
    await user.click(summary);
    expect(screen.getByText("Tool 5 — 1/8 endmill")).toBeVisible();
    expect(screen.getByText("Tool 6 — 6.35 mm flat end mill")).toBeVisible();
    expect(screen.getByText("Tool 9 (not in your tool library)")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Add to tool library" }));
    await user.click(screen.getByRole("button", { name: "Add to tool library" }));
    expect(await screen.findByText("Tool 6 — 6.35 mm flat end mill")).toBeInTheDocument();
    expect(screen.queryByText("(1 to review)")).not.toBeInTheDocument();
  });

  it("reads them again when the gcode changes, and shows nothing without a file", async () => {
    let fileTools = [fileTool(1, null)];
    const server = toolServer([endmill], [], {
      "GET /api/state": startupState({ gcodeFile: "part.nc" }),
      "/api/files/gcode/tools": () => new Response(JSON.stringify({ tools: fileTools })),
    });
    const { socket } = renderApp("/", { fetch: server.fetch });
    expect(await screen.findByText(/Tools: T1/)).toBeInTheDocument();
    fileTools = [fileTool(2, null), fileTool(3, null)];
    socket.serverMessage("gcodeUpdateCompressed", "");
    expect(await screen.findByText(/Tools: T2, T3/)).toBeInTheDocument();
    socket.serverMessage("gcodeFile", { file: null });
    await expect.poll(() => screen.queryByText(/Tools: T/)).toBeNull();
  });

  it("shows nothing when the file names no tools or they cannot be read", async () => {
    const server = toolServer([endmill], [], {
      "GET /api/state": startupState({ gcodeFile: "part.nc" }),
      "/api/files/gcode/tools": () => new Response("broken", { status: 500 }),
    });
    renderApp("/", { fetch: server.fetch });
    expect((await screen.findAllByText("part.nc")).length).toBeGreaterThan(0);
    expect(screen.queryByText(/Tools: T/)).not.toBeInTheDocument();
  });
});
