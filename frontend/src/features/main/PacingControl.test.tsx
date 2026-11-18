import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { PacingSegment, PacingStats } from "../../protocol";
import { fakeFetch, jsonResponse } from "../../test/fakeFetch";
import { renderWithProviders } from "../../test/render";
import { PacingControl } from "./PacingControl";
import { describe as describeSegment, presetFor, refusal, validGap } from "./pacing";

const segment = (over: Partial<PacingSegment> = {}): PacingSegment => ({
  mode: "v0.94",
  gapMs: 50,
  lines: 2284,
  lost: 9,
  lostTwice: 0,
  elapsedS: 270.3,
  recoveryS: 7.1,
  linesPerMinute: 507,
  secondsPerLine: 0.118,
  lostPercent: 0.39,
  medianShortGapMs: 62,
  endedBy: null,
  ...over,
});

const stats = (over: Partial<PacingStats> = {}): PacingStats => ({
  mode: "v0.94",
  gapMs: 50,
  job: null,
  segment: null,
  history: [],
  ...over,
});

const put = (body: (sent: unknown) => Response) => (init: RequestInit | undefined) =>
  body(JSON.parse(String(init?.body)));

const renderControl = async (loaded = stats(), routes: Parameters<typeof fakeFetch>[0] = {}) => {
  const view = renderWithProviders(<PacingControl />, {
    fetch: fakeFetch({ "/api/run/pacing": loaded, ...routes }),
  });
  await screen.findByRole("group", { name: "Line pacing" });
  return view;
};

describe("pacing helpers", () => {
  it("names the preset that matches, else custom", () => {
    expect(presetFor({ mode: "v0.94", gapMs: 50 })).toBe("v094-50");
    expect(presetFor({ mode: "minimum gap", gapMs: 0 })).toBe("fastest");
    expect(presetFor({ mode: "v0.94", gapMs: 25 })).toBe("custom");
  });

  it("accepts whole gaps from 0 to 500", () => {
    expect(["0", "500", "25"].map(validGap)).toEqual([0, 500, 25]);
    expect(["", "-1", "501", "2.5", "x"].map(validGap)).toEqual([null, null, null, null, null]);
  });

  it("describes a segment with its rate first and dashes for what is unknown", () => {
    expect(describeSegment(segment())).toEqual({
      rate: "507 lines/min",
      lost: "Lost 9 (0.39%)",
      detail: "2284 lines sent · Lost 9 (0.39%) · 0.118 s/line · 7.1 s recovering · v0.94 50 ms",
    });
    const empty = segment({
      lines: 0,
      lost: 0,
      lostTwice: 1,
      linesPerMinute: null,
      secondsPerLine: null,
      lostPercent: null,
    });
    expect(describeSegment({ ...empty, mode: "minimum gap", gapMs: 0, recoveryS: 0 })).toEqual({
      rate: "No lines yet",
      lost: "Lost 0",
      detail: "0 lines sent · Lost 0 · 1 lost twice · – s/line · 0.0 s recovering · min gap 0 ms",
    });
  });

  it("shows the server's reason for a refusal", () => {
    expect(refusal(new Error("boom"))).toBe("Could not change the pacing.");
  });
});

describe("PacingControl", () => {
  it("renders nothing until the pacing loads", () => {
    renderWithProviders(<PacingControl />);
    expect(screen.queryByRole("group", { name: "Line pacing" })).not.toBeInTheDocument();
  });

  it("shows the current preset and the running segment's rate", async () => {
    const { user } = await renderControl(stats({ segment: segment(), job: segment() }));
    expect(screen.getByLabelText("Pacing")).toHaveValue("v094-50");
    const button = screen.getByRole("button", { name: "507 lines/min, Lost 9 (0.39%)" });
    expect(button).toHaveAttribute("aria-haspopup", "dialog");
    expect(button).toHaveAttribute("title", expect.stringContaining("Lost 9 (0.39%) · 0.118 s/line"));
    await user.click(button);
    const dialog = screen.getByRole("dialog", { name: "Line pacing" });
    expect(dialog).toHaveTextContent("This segment: 507 lines/min · 2284 lines sent · Lost 9 (0.39%) · 0.118 s/line");
    expect(dialog).toHaveTextContent("This job: 507 lines/min");
  });

  it("shows the last job outside a job, and nothing before the first", async () => {
    const { socket, user } = await renderControl(stats({ job: segment({ endedBy: "job end" }) }));
    await user.click(screen.getByRole("button", { name: "507 lines/min, Last job · Lost 9 (0.39%)" }));
    expect(screen.getByRole("dialog", { name: "Line pacing" })).toHaveTextContent("Last job: 507 lines/min");
    socket.serverMessage("pacingStats", stats());
    expect(screen.getByRole("button", { name: "Pacing stats" })).not.toHaveAttribute("title");
  });

  it("follows the pacingStats pushes", async () => {
    const { socket } = await renderControl();
    socket.serverMessage(
      "pacingStats",
      stats({ mode: "minimum gap", gapMs: 25, segment: segment({ linesPerMinute: 612 }) }),
    );
    expect(screen.getByLabelText("Pacing")).toHaveValue("min-25");
    expect(screen.getByRole("button", { name: /612 lines\/min/ })).toBeInTheDocument();
  });

  it("keeps a push that lands while the pacing is still loading", async () => {
    const answers: ((response: Response) => void)[] = [];
    const pending = () => new Promise<Response>((resolve) => answers.push(resolve));
    const { socket } = renderWithProviders(<PacingControl />, { fetch: fakeFetch({ "/api/run/pacing": pending }) });
    await waitFor(() => expect(answers).toHaveLength(1));
    socket.serverMessage("pacingStats", stats({ mode: "minimum gap", gapMs: 25 }));
    answers[0](jsonResponse(stats()));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(screen.getByLabelText("Pacing")).toHaveValue("min-25");
  });

  it("saves a preset at once", async () => {
    const sent: unknown[] = [];
    const routes = {
      "PUT /api/run/pacing": put((body) => {
        sent.push(body);
        return jsonResponse(stats({ mode: "minimum gap", gapMs: 0 }));
      }),
    };
    const { user } = await renderControl(stats(), routes);
    await user.selectOptions(screen.getByLabelText("Pacing"), "Min 0 ms");
    await waitFor(() => expect(screen.getByLabelText("Pacing")).toHaveValue("fastest"));
    expect(sent).toEqual([{ values: { linePacingMode: "minimum gap", linePacingGapMs: 0 } }]);
  });

  it("applies a custom mode and gap, and refuses a gap out of range", async () => {
    const sent: unknown[] = [];
    const routes = {
      "PUT /api/run/pacing": put((body) => {
        sent.push(body);
        return jsonResponse(stats({ mode: "v0.94", gapMs: 75 }));
      }),
    };
    const { user } = await renderControl(stats(), routes);
    await user.selectOptions(screen.getByLabelText("Pacing"), "Custom…");
    const gap = screen.getByRole("spinbutton", { name: "Gap in milliseconds" });
    await user.clear(gap);
    await user.type(gap, "501");
    expect(screen.getByRole("button", { name: "Apply" })).toBeDisabled();
    await user.clear(gap);
    await user.type(gap, "75");
    await user.click(screen.getByRole("button", { name: "Apply" }));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Apply" })).not.toBeInTheDocument());
    expect(sent).toEqual([{ values: { linePacingMode: "v0.94", linePacingGapMs: 75 } }]);
    expect(screen.getByRole("option", { name: "v0.94 75 ms" })).toBeInTheDocument();
  });

  it("shows why the server refused a change", async () => {
    const routes = {
      "PUT /api/run/pacing": () => jsonResponse({ errors: { linePacingGapMs: "Must be between 0 and 500" } }, 400),
    };
    const { user } = await renderControl(stats(), routes);
    await user.selectOptions(screen.getByLabelText("Pacing"), "Custom…");
    await user.selectOptions(screen.getByRole("combobox", { name: "Pacing mode" }), "minimum gap");
    await user.click(screen.getByRole("button", { name: "Apply" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Must be between 0 and 500");
  });

  it("lists the ended segments side by side in the history", async () => {
    const history = [
      segment({ mode: "minimum gap", gapMs: 25, endedBy: "job end", linesPerMinute: 612 }),
      segment({ endedBy: "pacing change", medianShortGapMs: null }),
    ];
    const { user } = await renderControl(stats({ history }));
    await user.click(screen.getByRole("button", { name: "Pacing stats" }));
    const dialog = screen.getByRole("dialog", { name: "Line pacing" });
    const rows = within(dialog).getAllByRole("row");
    expect(within(rows[1]).getByRole("rowheader")).toHaveTextContent("min gap 25 ms");
    expect(rows[1]).toHaveTextContent("612");
    expect(rows[2]).toHaveTextContent("pacing change");
    const scroller = within(dialog).getByRole("region", { name: "Segment history" });
    expect(scroller).toHaveAttribute("tabindex", "0");
    expect(scroller).toContainElement(rows[0]);
    expect(within(rows[0]).getByTitle("Recovery, seconds")).toHaveTextContent("Recov. s");
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("names a segment with no lines yet without a rate", async () => {
    await renderControl(stats({ segment: segment({ lines: 0, linesPerMinute: null, lostPercent: null, lost: 0 }) }));
    expect(screen.getByRole("button", { name: "No lines yet, Lost 0" })).toBeInTheDocument();
  });

  it("says when no segment has ended yet", async () => {
    const { user } = await renderControl();
    await user.click(screen.getByRole("button", { name: "Pacing stats" }));
    expect(screen.getByText(/No pacing segment has ended yet/)).toBeInTheDocument();
  });
});
