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
      detail: "Sent 2284 · Lost 9 (0.39%) · 0.118 s/line · 7.1 s recovering · v0.94 50 ms",
    });
    const empty = segment({
      lines: 0,
      lost: 0,
      lostTwice: 1,
      linesPerMinute: null,
      secondsPerLine: null,
      lostPercent: null,
    });
    expect(describeSegment({ ...empty, mode: "minimum gap", gapMs: 0, recoveryS: 0 }).detail).toBe(
      "Sent 0 · Lost 0 (–%) · 1 lost twice · – s/line · 0.0 s recovering · min gap 0 ms",
    );
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
    await renderControl(stats({ segment: segment(), job: segment() }));
    expect(screen.getByLabelText("Pacing")).toHaveValue("v094-50");
    expect(screen.getByText("507 lines/min")).toBeInTheDocument();
    expect(screen.getByText(/Sent 2284 · Lost 9 \(0\.39%\) · 0\.118 s\/line/)).toBeInTheDocument();
  });

  it("shows the last job outside a job, and says when there is none", async () => {
    const { socket } = await renderControl(stats({ job: segment({ endedBy: "job end" }) }));
    expect(screen.getByText(/Last job:/)).toBeInTheDocument();
    socket.serverMessage("pacingStats", stats());
    expect(screen.getByText("No job yet")).toBeInTheDocument();
  });

  it("follows the pacingStats pushes", async () => {
    const { socket } = await renderControl();
    socket.serverMessage(
      "pacingStats",
      stats({ mode: "minimum gap", gapMs: 25, segment: segment({ linesPerMinute: 612 }) }),
    );
    expect(screen.getByLabelText("Pacing")).toHaveValue("min-25");
    expect(screen.getByText("612 lines/min")).toBeInTheDocument();
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
    await user.selectOptions(screen.getByLabelText("Pacing"), "Fastest · 0 ms");
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
    expect(screen.getByRole("option", { name: "Custom · v0.94 75 ms" })).toBeInTheDocument();
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
    await user.click(screen.getByRole("button", { name: "History" }));
    const dialog = screen.getByRole("dialog", { name: "Pacing history" });
    const rows = within(dialog).getAllByRole("row");
    expect(within(rows[1]).getByRole("rowheader")).toHaveTextContent("min gap 25 ms");
    expect(rows[1]).toHaveTextContent("612");
    expect(rows[2]).toHaveTextContent("pacing change");
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("says when no segment has ended yet", async () => {
    const { user } = await renderControl();
    await user.click(screen.getByRole("button", { name: "History" }));
    expect(screen.getByText(/No pacing segment has ended yet/)).toBeInTheDocument();
  });
});
