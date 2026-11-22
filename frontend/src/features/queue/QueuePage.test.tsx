import { act, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { JobQueue } from "../../protocol";
import { fakeFetch, jsonResponse, startupState } from "../../test/fakeFetch";
import { deferred, gcodeLibrary } from "../../test/filesBoards";
import { jobQueue, queueItem } from "../../test/queueFixtures";
import { renderApp, renderWithProviders } from "../../test/render";
import { QueueSummary } from "./QueuePage";

const three = jobQueue({
  state: "running",
  headLocked: true,
  items: [
    queueItem({ id: 1, file: "jobs/leg.nc" }),
    queueItem({ id: 2, file: "shelf.nc" }),
    queueItem({ id: 3, file: "jobs/sign.nc", tool: 2, between: "toolChange" }),
  ],
});

/** Records each POST body and answers with `answer`. */
const posts = (answer: (body: unknown) => Response = () => jsonResponse(three)) => {
  const sent: unknown[] = [];
  const respond = (init: RequestInit | undefined) => {
    const body: unknown = JSON.parse(String(init?.body));
    sent.push(body);
    return answer(body);
  };
  return { sent, respond };
};

const routes = (queue: JobQueue, write = posts()) => ({
  "/api/queue": queue,
  "POST /api/queue/add": write.respond,
  "POST /api/queue/move": write.respond,
  "POST /api/queue/remove": write.respond,
  "POST /api/queue/clear": write.respond,
  "POST /api/queue/continue": write.respond,
});

describe("Job queue screen", () => {
  it("lists the files in order with what happens before each", async () => {
    renderApp("/queue", { fetch: fakeFetch(routes(three)) });
    const list = await screen.findByRole("list", { name: "Queued files" });
    const rows = within(list).getAllByRole("listitem");
    expect(rows.map((row) => row.textContent)).toEqual([
      expect.stringContaining("leg.nc in jobsNowLoaded for the job"),
      expect.stringContaining("shelf.ncNextsame tool, starts immediately"),
      expect.stringContaining("sign.nc in jobsTool 2, will pause for a tool change"),
    ]);
    expect(screen.getByText("Running leg.nc.", { selector: ".queue-status" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Remove jobs/leg.nc" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Move shelf.nc up" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Move jobs/sign.nc down" })).toBeDisabled();
  });

  it("moves, removes and clears through the server", async () => {
    const write = posts();
    const { user } = renderApp("/queue", { fetch: fakeFetch(routes(three, write)) });
    await user.click(await screen.findByRole("button", { name: "Move jobs/sign.nc up" }));
    await user.click(screen.getByRole("button", { name: "Move shelf.nc down" }));
    await user.click(screen.getByRole("button", { name: "Remove shelf.nc" }));
    await user.click(screen.getByRole("button", { name: "Clear queue" }));
    expect(write.sent).toEqual([{ id: 3, to: 1 }, { id: 2, to: 2 }, { id: 2 }, {}]);
  });

  it("adds the loaded file and shows a refusal", async () => {
    const write = posts(() => jsonResponse({ error: "The queue holds 100 files at most." }, 409));
    const fetch = fakeFetch({ ...routes(jobQueue(), write), "GET /api/state": startupState({ gcodeFile: "a.nc" }) });
    const { user } = renderApp("/queue", { fetch });
    expect(await screen.findByText(/No files queued/)).toBeInTheDocument();
    await user.click(await screen.findByRole("button", { name: "Add loaded file" }));
    expect(write.sent).toEqual([{ file: "a.nc" }]);
    expect(await screen.findByRole("alert")).toHaveTextContent("The queue holds 100 files at most.");
    expect(screen.getByRole("button", { name: "Start queue" })).toBeDisabled();
  });

  it("starts the queue, and follows the pushes", async () => {
    const write = posts();
    const idle = jobQueue({ items: [queueItem()] });
    const { user, socket } = renderApp("/queue", { fetch: fakeFetch(routes(idle, write)) });
    await user.click(await screen.findByRole("button", { name: "Start queue" }));
    expect(write.sent).toEqual([{}]);
    act(() => socket.serverMessage("jobQueue", { ...idle, state: "stopped", reason: "Stop was pressed." }));
    expect(screen.getByText("Queue stopped: Stop was pressed.", { selector: ".queue-status" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Continue queue" })).toBeEnabled();
  });

  it("keeps a push that came before its own write's answer", async () => {
    const answer = deferred();
    const idle = jobQueue({ items: [queueItem()] });
    const fetch = fakeFetch({ ...routes(idle), "POST /api/queue/continue": answer.respond });
    const { user, socket } = renderApp("/queue", { fetch });
    await user.click(await screen.findByRole("button", { name: "Start queue" }));
    act(() => socket.serverMessage("jobQueue", { ...idle, state: "running", step: "raising", headLocked: true }));
    await act(async () => answer.settle.resolve(jsonResponse({ ...idle, state: "running", step: "loading" })));
    expect(screen.getByText("Raising Z before leg.nc…", { selector: ".queue-status" })).toBeInTheDocument();
  });

  it("is in the G-Code menu", async () => {
    const { user } = renderApp("/", { fetch: fakeFetch(routes(three)) });
    await user.click(screen.getByRole("button", { name: "G-Code" }));
    await user.click(screen.getByRole("menuitem", { name: "Job Queue" }));
    expect(await screen.findByRole("heading", { name: "Job Queue" })).toBeInTheDocument();
  });
});

describe("Queue summary on the control screen", () => {
  it.each([
    ["the queue is empty", fakeFetch({ "/api/queue": jobQueue() })],
    ["the backend has no queue", fakeFetch()],
  ])("stays hidden while %s", async (_, fetch) => {
    renderWithProviders(<QueueSummary />, { fetch });
    await act(async () => {});
    expect(fetch).toHaveBeenCalledWith("/api/queue", expect.anything());
    expect(screen.queryByRole("region", { name: "Job queue" })).not.toBeInTheDocument();
  });

  it("says what comes next and opens the queue", async () => {
    const { user } = renderApp("/", { fetch: fakeFetch(routes(three)) });
    const summary = await screen.findByRole("region", { name: "Job queue" });
    expect(summary).toHaveTextContent("Next: shelf.nc — same tool, starts immediately");
    await user.click(within(summary).getByRole("button", { name: "Queue (3)" }));
    expect(await screen.findByRole("list", { name: "Queued files" })).toBeInTheDocument();
  });

  it("says why it stopped and continues in one click", async () => {
    const stopped = jobQueue({ state: "stopped", reason: "Could not load sign.nc.", items: [queueItem()] });
    const write = posts(() => jsonResponse({ error: "The controller is not connected." }, 409));
    const { user } = renderWithProviders(<QueueSummary />, { fetch: fakeFetch(routes(stopped, write)) });
    const summary = await screen.findByRole("region", { name: "Job queue" });
    expect(within(summary).getByRole("status")).toHaveTextContent("Queue stopped: Could not load sign.nc.");
    expect(summary).toHaveTextContent("First: leg.nc — same tool, starts immediately");
    await user.click(within(summary).getByRole("button", { name: "Continue queue" }));
    expect(write.sent).toEqual([{}]);
    expect(await within(summary).findByRole("alert")).toHaveTextContent("The controller is not connected.");
  });
});

describe("Add to queue from Open G-Code", () => {
  it("adds the chosen file without opening it", async () => {
    const write = posts();
    const fetch = fakeFetch({ "/api/files/gcode": gcodeLibrary(), ...routes(jobQueue(), write) });
    const { user } = renderApp("/files/open", { fetch });
    await user.click(await screen.findByRole("radio", { name: "arm.nc" }));
    await user.click(screen.getByRole("button", { name: "Add to queue" }));
    expect(write.sent).toEqual([{ file: "jobs/arm.nc" }]);
    expect(await screen.findByText("Added arm.nc to the queue.")).toBeInTheDocument();
  });

  it("explains a refusal", async () => {
    const write = posts(() => jsonResponse({ error: "No such file" }, 404));
    const fetch = fakeFetch({ "/api/files/gcode": gcodeLibrary({ lastFile: "" }), ...routes(jobQueue(), write) });
    const { user } = renderApp("/files/open", { fetch });
    await user.click(await screen.findByRole("button", { name: "Add to queue" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Choose a file to add.");
    await user.click(screen.getByRole("radio", { name: "arm.nc" }));
    await user.click(screen.getByRole("button", { name: "Add to queue" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not add arm.nc to the queue.");
  });
});
