import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeFetch } from "../../test/fakeFetch";
import { currentPath, renderApp } from "../../test/render";

const loaded = (over = {}) => ({ directory: "jobs", name: "leg.nc", lineCount: 1234, ...over });
const panel = (name: string) => within(screen.getByRole("region", { name }));

describe("Files home", () => {
  it("names the loaded file and its line count", async () => {
    renderApp("/files", { fetch: fakeFetch({ "/api/files/gcode/loaded": loaded() }) });
    expect(screen.getByRole("heading", { name: "Files", level: 1 })).toBeInTheDocument();
    expect(await panel("Loaded gcode").findByText("jobs/leg.nc")).toBeInTheDocument();
    expect(panel("Loaded gcode").getByText(/1,234 lines/)).toBeInTheDocument();
  });

  it.each([
    [loaded({ directory: ".", name: "part.nc", lineCount: 1 }), "part.nc — 1 line"],
    [loaded({ directory: null, name: "part.nc" }), "part.nc — 1,234 lines"],
    [loaded({ directory: null, name: null, lineCount: 3 }), "Edited gcode — 3 lines"],
    [loaded({ directory: null, name: null, lineCount: 0 }), "No gcode is loaded."],
  ])("summarises %o", async (body, text) => {
    renderApp("/files", { fetch: fakeFetch({ "/api/files/gcode/loaded": body }) });
    await waitFor(() =>
      expect(
        panel("Loaded gcode").getByText((_, el) => el?.tagName === "P" && el.textContent === text),
      ).toBeInTheDocument(),
    );
  });

  it("reloads the summary whenever the server pushes new gcode", async () => {
    let lineCount = 10;
    const fetch = fakeFetch({ "/api/files/gcode/loaded": () => Response.json(loaded({ lineCount })) });
    const { socket } = renderApp("/files", { fetch });
    expect(await screen.findByText(/10 lines/)).toBeInTheDocument();

    lineCount = 0;
    socket.serverRaw({ command: "gcodeUpdateCompressed", data: "", dataFormat: "base64" });
    expect(await screen.findByText("No gcode is loaded.")).toBeInTheDocument();
  });

  it("says so when the summary cannot load", async () => {
    renderApp("/files");
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not load the loaded gcode.");
  });

  it("clears the loaded gcode", async () => {
    const { socket, user } = renderApp("/files", { fetch: fakeFetch({ "/api/files/gcode/loaded": loaded() }) });
    await user.click(screen.getByRole("button", { name: "Clear gcode" }));
    expect(socket.actions()).toEqual([["clearGCode", "", ""]]);
  });

  it("downloads diagnostics and backups from the classic routes", () => {
    renderApp("/files", { fetch: fakeFetch({ "/api/files/gcode/loaded": loaded() }) });
    const backup = panel("Backup and import");
    expect(backup.getByRole("link", { name: "Download diagnostics" })).toHaveAttribute("href", "/downloadDiagnostics");
    expect(backup.getByRole("link", { name: "Back up WebControl" })).toHaveAttribute("download");
  });

  it.each([
    ["Open…", "/files/open", "Open gcode"],
    ["Upload…", "/files/upload", "Upload gcode"],
    ["Save as…", "/files/save", "Save gcode as"],
    ["View or edit…", "/files/edit", "View or edit gcode"],
    ["Send gcode…", "/files/send", "Send gcode"],
    ["Restore WebControl…", "/files/import/restore", "Restore WebControl"],
    ["Import groundcontrol.ini…", "/files/import/groundcontrol", "Import groundcontrol.ini"],
    ["Import webcontrol.json…", "/files/import/webcontrol", "Import webcontrol.json"],
  ])("%s opens its own screen and Back returns", async (link, path, heading) => {
    const { user } = renderApp("/files", { fetch: fakeFetch({ "/api/files/gcode/loaded": loaded() }) });
    await user.click(screen.getByRole("link", { name: link }));
    expect(currentPath()).toBe(path);
    expect(screen.getByRole("heading", { name: heading, level: 2 })).toBeInTheDocument();

    await user.click(screen.getByRole("link", { name: "Back" }));
    expect(currentPath()).toBe("/files");
  });

  it("sends unknown sub-paths back to the files screen", () => {
    renderApp("/files/nope", { fetch: fakeFetch({ "/api/files/gcode/loaded": loaded() }) });
    expect(currentPath()).toBe("/files");
  });
});
