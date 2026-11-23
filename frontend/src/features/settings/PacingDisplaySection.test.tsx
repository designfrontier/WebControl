import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { PacingStats } from "../../protocol";
import { fakeFetch, jsonResponse, startupState } from "../../test/fakeFetch";
import { renderApp } from "../../test/render";

const pacing = (showStats?: boolean): PacingStats => ({
  mode: "v0.94",
  gapMs: 50,
  job: null,
  segment: null,
  history: [],
  showStats,
});

const pacingServer = (shown: boolean | undefined, status = 200) => {
  const bodies: unknown[] = [];
  const fetch = fakeFetch({
    "GET /api/state": startupState({ pacing: pacing(shown) }),
    "/api/run/pacing": pacing(shown),
    "PUT /api/run/pacing": (init) => {
      const body = JSON.parse(String(init?.body)) as { values: { linePacingStats: boolean } };
      bodies.push(body);
      return jsonResponse(status === 200 ? pacing(body.values.linePacingStats) : { error: "No" }, status);
    },
  });
  return { fetch, bodies };
};

const toggle = () =>
  within(screen.getByRole("group", { name: "Line pacing" })).getByRole("checkbox", {
    name: "Show line pacing in the run panel",
  });

describe("Line pacing display setting", () => {
  it("is off by default, and the run panel shows no pacing", async () => {
    renderApp("/settings/display", { fetch: pacingServer(undefined).fetch });
    expect(await screen.findByRole("heading", { name: "Display", level: 3 })).toBeInTheDocument();
    expect(toggle()).not.toBeChecked();
    expect(screen.getByRole("group", { name: "Line pacing" })).toHaveAccessibleDescription(/on every device/);
    expect(screen.queryByRole("combobox", { name: "Pacing", hidden: true })).not.toBeInTheDocument();
  });

  it("turns it on through the pacing endpoint, and the run panel shows the picker", async () => {
    const { fetch, bodies } = pacingServer(false);
    const { user, socket } = renderApp("/settings/display", { fetch });
    await waitFor(() => expect(toggle()).toBeEnabled());
    await user.click(toggle());
    expect(bodies).toEqual([{ values: { linePacingStats: true } }]);
    await waitFor(() => expect(toggle()).toBeChecked());
    socket.serverMessage("pacingStats", pacing(true));
    expect(await screen.findByRole("combobox", { name: "Pacing", hidden: true })).toBeInTheDocument();
  });

  it("says when the change could not be saved", async () => {
    const { fetch } = pacingServer(true, 500);
    const { user } = renderApp("/settings/display", { fetch });
    await waitFor(() => expect(toggle()).toBeChecked());
    await user.click(toggle());
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not save the line pacing display.");
    expect(toggle()).toBeChecked();
  });
});
