import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeFetch, jsonResponse } from "../../test/fakeFetch";
import { topLayer } from "../../test/dialogPolyfill";
import { deferred, expectPath, gcodeLibrary, recorder } from "../../test/filesBoards";
import { renderApp } from "../../test/render";

const nc = (name: string) => new File(["G0 X0\n"], name, { type: "text/plain" });

const setup = (upload = recorder(), directories: Parameters<typeof fakeFetch>[0] = {}) => {
  const fetch = fakeFetch({ "/api/files/gcode": gcodeLibrary(), "POST /uploadGCode": upload.respond, ...directories });
  return { ...renderApp("/files/upload", { fetch }), upload, fetch };
};

describe("Upload gcode", () => {
  it("accepts the configured extensions into the last directory", async () => {
    setup();
    expect(await screen.findByLabelText("Gcode files")).toHaveAttribute("accept", ".nc, .gcode");
    expect(screen.getByLabelText("Directory")).toHaveValue("jobs");
  });

  it("needs a file", async () => {
    const { user, upload } = setup();
    await user.click(await screen.findByRole("button", { name: "Upload and open" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Choose at least one gcode file to upload.");
    expect(upload.calls).toEqual([]);
  });

  it("uploads every chosen file to the chosen directory and opens the first", async () => {
    const { user, upload, socket } = setup();
    await user.selectOptions(await screen.findByLabelText("Directory"), "Top level");
    await user.upload(screen.getByLabelText("Gcode files"), [nc("a.nc"), nc("b.nc")]);
    expect(screen.getByText("2 files selected. All are uploaded and a.nc is opened.")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Upload and open" }));

    const [form] = upload.forms();
    expect(form.selectedDirectory).toBe(".");
    expect((form["file[]"] as File[]).map((file) => file.name)).toEqual(["a.nc", "b.nc"]);
    await expectPath("/");
    expect(socket.emittedEvents("checkForGCodeUpdate")).toEqual([{ data: "Please" }]);
  });

  it("explains a failed upload", async () => {
    const { user } = setup(recorder(500));
    await user.upload(await screen.findByLabelText("Gcode files"), nc("a.nc"));
    await user.click(screen.getByRole("button", { name: "Upload and open" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("The upload failed");
    await expectPath("/files/upload");
  });

  it("covers the page with a loading overlay until the upload is opened", async () => {
    const answer = deferred();
    const { user } = setup({ respond: answer.respond } as unknown as ReturnType<typeof recorder>);
    await user.upload(await screen.findByLabelText("Gcode files"), nc("a.nc"));
    await user.click(screen.getByRole("button", { name: "Upload and open" }));
    expect(screen.getByRole("button", { name: "Uploading…" })).toBeDisabled();
    const overlay = screen.getByRole("dialog", { name: "Uploading gcode…" });
    expect(overlay).toHaveAccessibleDescription(/building its preview/);
    expect(topLayer().at(-1)).toBe(overlay);

    answer.settle.resolve(jsonResponse({ status: 200 }));
    await expectPath("/");
    expect(screen.queryByRole("dialog", { name: "Uploading gcode…" })).not.toBeInTheDocument();
  });

  it("clears the overlay when the upload fails", async () => {
    const answer = deferred();
    const { user } = setup({ respond: answer.respond } as unknown as ReturnType<typeof recorder>);
    await user.upload(await screen.findByLabelText("Gcode files"), nc("a.nc"));
    await user.click(screen.getByRole("button", { name: "Upload and open" }));
    expect(screen.getByRole("dialog", { name: "Uploading gcode…" })).toBeInTheDocument();

    answer.settle.reject(new TypeError("Failed to fetch"));
    expect(await screen.findByRole("alert")).toHaveTextContent("The upload failed");
    expect(screen.queryByRole("dialog", { name: "Uploading gcode…" })).not.toBeInTheDocument();
  });

  it("creates a directory and selects it", async () => {
    const created = { directory: "My_Parts", directories: [".", "My_Parts", "empty", "jobs"], files: [] };
    const { user, fetch } = setup(recorder(), {
      "POST /api/files/gcode/directories": () => jsonResponse(created, 201),
    });
    await user.type(await screen.findByLabelText("New directory"), "My Parts");
    await user.click(screen.getByRole("button", { name: "Create directory" }));

    expect(await screen.findByRole("option", { name: "My_Parts" })).toHaveProperty("selected", true);
    expect(screen.getByLabelText("New directory")).toHaveValue("");
    const call = fetch.mock.calls.find(([url]) => url === "/api/files/gcode/directories");
    expect(JSON.parse(call?.[1]?.body as string)).toEqual({ name: "My Parts" });
  });

  it("asks for a directory name, and explains names the server rejects", async () => {
    const { user } = setup(recorder(), {
      "POST /api/files/gcode/directories": () => jsonResponse({ error: "bad" }, 400),
    });
    await user.click(await screen.findByRole("button", { name: "Create directory" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Enter a name for the new directory.");

    await user.type(screen.getByLabelText("New directory"), "../");
    await user.click(screen.getByRole("button", { name: "Create directory" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not create that directory.");
  });
});
