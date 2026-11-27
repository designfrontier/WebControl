import { act, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { topLayer } from "../../test/dialogPolyfill";
import { fakeFetch, jsonResponse } from "../../test/fakeFetch";
import { deferred, expectPath, gcodeLibrary, pending, recorder } from "../../test/filesBoards";
import { currentPath, renderApp } from "../../test/render";
import { GCODE_LOAD } from "./hooks";

const setup = (open = recorder(), library = gcodeLibrary()) => {
  const fetch = fakeFetch({ "/api/files/gcode": library, "POST /openGCode": open.respond });
  return { ...renderApp("/files/open", { fetch }), open };
};

const warning =
  "This file has 1,200,000 lines and needs about 401 MB of the 700 MB this computer has free. Consider splitting it into smaller files and running them one after another with the job queue.";
const refusal =
  "This file has 3,000,000 lines and needs about 1002 MB of the 700 MB this computer has free. It was not opened.";

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
    expect(screen.queryByText(/building its preview/)).not.toBeInTheDocument();
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

  it("opens a file that strains the host's memory but stays to show why", async () => {
    const { user, socket } = setup(recorder(200, { status: 200, warning }));
    await user.click(await screen.findByRole("button", { name: "Open" }));
    expect(await screen.findByText(warning)).toHaveAttribute("role", "status");
    await expectPath("/files/open");
    expect(socket.emittedEvents("checkForGCodeUpdate")).toEqual([{ data: "Please" }]);
  });

  it("shows why the server refused a file too large for its memory", async () => {
    const { user } = setup(recorder(409, { status: 409, error: refusal }));
    await user.click(await screen.findByRole("button", { name: "Open" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(refusal);
  });

  it("covers the whole page with a loading overlay until the server answers", async () => {
    const answer = deferred();
    const { user } = setup({ respond: answer.respond } as unknown as ReturnType<typeof recorder>);
    await user.click(await screen.findByRole("button", { name: "Open" }));

    const overlay = screen.getByRole("dialog", { name: "Opening G-Code…" });
    expect(overlay).toHaveAccessibleDescription(/building its preview/);
    expect(topLayer().at(-1)).toBe(overlay);
    expect(overlay).toHaveFocus();
    expect(screen.getByRole("button", { name: "Opening…" })).toBeDisabled();

    await user.keyboard("{Escape}");
    expect(overlay).toBeInTheDocument();
    expect(currentPath()).toBe("/files/open");

    answer.settle.resolve(jsonResponse({ status: 200 }));
    await expectPath("/");
    expect(screen.queryByRole("dialog", { name: "Opening G-Code…" })).not.toBeInTheDocument();
    expect(topLayer()).toEqual([]);
  });

  describe("clears the overlay and explains when the load fails", () => {
    const failsWith = async (respond: () => Response | Promise<Response>, message: string | RegExp) => {
      const { user } = setup({ respond } as unknown as ReturnType<typeof recorder>);
      await user.click(await screen.findByRole("button", { name: "Open" }));
      expect(await screen.findByRole("alert")).toHaveTextContent(message);
      expect(screen.queryByRole("dialog", { name: "Opening G-Code…" })).not.toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Open" })).toBeEnabled();
      expect(currentPath()).toBe("/files/open");
    };

    it("on a network failure", () =>
      failsWith(() => Promise.reject(new TypeError("Failed to fetch")), "Could not open leg.nc."));

    it("on an error status without a body", () =>
      failsWith(() => new Response("Bad Gateway", { status: 502 }), "Could not open leg.nc."));

    it("on an error body of the wrong shape", () =>
      failsWith(() => jsonResponse({ error: 42 }, 500), "Could not open leg.nc."));

    it("on an error thrown after the server answered", async () => {
      const fetch = fakeFetch({ "/api/files/gcode": gcodeLibrary(), "POST /openGCode": { status: 200 } });
      const { user, socket } = renderApp("/files/open", { fetch });
      const open = await screen.findByRole("button", { name: "Open" });
      vi.spyOn(socket, "emit").mockImplementation(() => {
        throw new Error("socket gone");
      });
      await user.click(open);
      expect(await screen.findByRole("alert")).toHaveTextContent("Could not open leg.nc.");
      expect(screen.queryByRole("dialog", { name: "Opening G-Code…" })).not.toBeInTheDocument();
    });
  });

  it("drops the overlay but lets the server finish when the screen closes mid-load, as Back does", async () => {
    const answer = deferred();
    const fetch = fakeFetch({ "/api/files/gcode": gcodeLibrary(), "POST /openGCode": answer.respond });
    const { user, socket } = renderApp("/files/open", { fetch });
    await user.click(await screen.findByRole("button", { name: "Open" }));
    expect(screen.getByRole("dialog", { name: "Opening G-Code…" })).toBeInTheDocument();

    act(() => screen.getByRole("button", { name: "Cancel" }).click());
    await expectPath("/");
    expect(screen.queryByRole("dialog", { name: "Opening G-Code…" })).not.toBeInTheDocument();
    expect(topLayer()).toEqual([]);
    expect(fetch.mock.calls.find(([path]) => path === "/openGCode")?.[1]?.signal).toBeUndefined();

    await act(async () => answer.settle.resolve(jsonResponse({ status: 200 })));
    expect(socket.emittedEvents("checkForGCodeUpdate")).toHaveLength(1);
    expect(currentPath()).toBe("/");
  });

  describe("when the server never answers", () => {
    afterEach(() => vi.useRealTimers());

    it("gives up after the timeout, clears the overlay and says so", async () => {
      vi.useFakeTimers({ shouldAdvanceTime: true });
      const fetch = fakeFetch({ "/api/files/gcode": gcodeLibrary(), "POST /openGCode": pending });
      const { user } = renderApp("/files/open", { fetch });
      await user.click(await screen.findByRole("button", { name: "Open" }));
      expect(screen.getByRole("dialog", { name: "Opening G-Code…" })).toBeInTheDocument();

      await act(() => vi.advanceTimersByTimeAsync(GCODE_LOAD.timeoutMs));
      expect(screen.getByRole("alert")).toHaveTextContent(GCODE_LOAD.timeoutMessage);
      expect(screen.queryByRole("dialog", { name: "Opening G-Code…" })).not.toBeInTheDocument();
      expect(currentPath()).toBe("/files/open");
    });

    it("still shows a load that finishes after the timeout, without closing the screen", async () => {
      vi.useFakeTimers({ shouldAdvanceTime: true });
      const answer = deferred();
      const fetch = fakeFetch({ "/api/files/gcode": gcodeLibrary(), "POST /openGCode": answer.respond });
      const { user, socket } = renderApp("/files/open", { fetch });
      await user.click(await screen.findByRole("button", { name: "Open" }));
      await act(() => vi.advanceTimersByTimeAsync(GCODE_LOAD.timeoutMs));

      await act(async () => answer.settle.resolve(jsonResponse({ status: 200 })));
      expect(socket.emittedEvents("checkForGCodeUpdate")).toHaveLength(1);
      expect(currentPath()).toBe("/files/open");
    });
  });

  describe("delete", () => {
    const afterDelete = { directories: [".", "empty"], files: [{ directory: ".", name: "part.nc" }] };
    const setupDelete = (remove = recorder(200, afterDelete)) => {
      const fetch = fakeFetch({ "/api/files/gcode": gcodeLibrary(), "POST /api/files/gcode/delete": remove.respond });
      return { ...renderApp("/files/open", { fetch }), remove };
    };
    const sent = (remove: ReturnType<typeof recorder>) => remove.calls.map((init) => JSON.parse(String(init?.body)));

    it("deletes the selected file after confirming", async () => {
      const { user, remove } = setupDelete(
        recorder(200, gcodeLibrary({ files: [{ directory: "jobs", name: "arm.nc" }] })),
      );
      await user.click(await screen.findByRole("button", { name: "Delete file" }));
      const confirm = screen.getByRole("dialog", { name: "Delete from the library" });
      expect(confirm).toHaveTextContent("Delete leg.nc? This cannot be undone.");
      await user.click(within(confirm).getByRole("button", { name: "Delete" }));

      expect(sent(remove)).toEqual([{ path: "jobs/leg.nc" }]);
      await waitFor(() => expect(screen.queryByRole("radio", { name: "leg.nc" })).not.toBeInTheDocument());
      expect(screen.getByRole("radio", { name: "arm.nc" })).not.toBeChecked();
      expect(currentPath()).toBe("/files/open");
    });

    it("says how many files go with a directory, then shows the top level", async () => {
      const { user, remove } = setupDelete();
      await user.click(await screen.findByRole("button", { name: "Delete directory" }));
      const confirm = screen.getByRole("dialog", { name: "Delete from the library" });
      expect(confirm).toHaveTextContent("Delete the directory jobs and the 2 files in it?");
      await user.click(within(confirm).getByRole("button", { name: "Delete" }));

      expect(sent(remove)).toEqual([{ path: "jobs" }]);
      expect(await screen.findByRole("radio", { name: "part.nc" })).toBeInTheDocument();
      expect(screen.getByLabelText("Directory")).toHaveValue(".");
      expect(screen.queryByRole("option", { name: "jobs" })).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Delete directory" })).not.toBeInTheDocument();
    });

    it("deletes nothing when cancelled", async () => {
      const { user, remove } = setupDelete();
      await user.click(await screen.findByRole("button", { name: "Delete directory" }));
      const confirm = screen.getByRole("dialog", { name: "Delete from the library" });
      await user.click(within(confirm).getByRole("button", { name: "Cancel" }));
      expect(screen.queryByRole("dialog", { name: "Delete from the library" })).not.toBeInTheDocument();
      expect(remove.calls).toEqual([]);
      expect(screen.getByRole("radio", { name: "leg.nc" })).toBeChecked();
    });

    it("asks for a file first", async () => {
      const { user, remove } = setupDelete();
      await screen.findByRole("radio", { name: "leg.nc" });
      await user.selectOptions(screen.getByLabelText("Directory"), "Top level");
      await user.click(screen.getByRole("button", { name: "Delete file" }));
      expect(screen.getByRole("alert")).toHaveTextContent("Choose a file to delete.");
      expect(remove.calls).toEqual([]);
    });

    it("shows the server's reason for refusing", async () => {
      const error = "jobs/leg.nc is, or holds, the loaded gcode or a file in the job queue, so it stays.";
      const { user } = setupDelete(recorder(409, { error }));
      await user.click(await screen.findByRole("button", { name: "Delete file" }));
      await user.click(screen.getByRole("button", { name: "Delete" }));
      expect(await screen.findByRole("alert")).toHaveTextContent(error);
      expect(screen.getByRole("radio", { name: "leg.nc" })).toBeChecked();
    });

    it("falls back to its own message when the request fails", async () => {
      const { user } = setupDelete({ respond: () => Promise.reject(new TypeError("offline")) } as unknown as ReturnType<
        typeof recorder
      >);
      await user.click(await screen.findByRole("button", { name: "Delete directory" }));
      await user.click(screen.getByRole("button", { name: "Delete" }));
      expect(await screen.findByRole("alert")).toHaveTextContent("Could not delete jobs.");
    });
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
