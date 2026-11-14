import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { jsonResponse } from "../../test/fakeFetch";
import { renderApp } from "../../test/render";
import { releases, systemFetch } from "./systemFixtures";

const release = (name: string) => within(screen.getByRole("heading", { name, level: 3 }).closest("li")!);

const renderReleases = (routes = {}, options: Parameters<typeof renderApp>[1] = {}) =>
  renderApp("/system/updates", { fetch: systemFetch(routes), ...options });

describe("ReleasesPanel", () => {
  it("lists releases, marking the installed and latest ones", async () => {
    renderReleases();
    expect(await screen.findByText("Installed version: v0.95")).toBeInTheDocument();
    expect(release("Faster previews (v0.96)").getByText("Latest")).toBeInTheDocument();
    expect(release("Faster previews (v0.96)").getByRole("link", { name: "#12" })).toHaveAttribute("target", "_blank");
    expect(release("v0.95").getByText("Installed")).toBeInTheDocument();
    expect(release("v0.95").queryByRole("button")).not.toBeInTheDocument();
    expect(release("Older (v0.94)").getByRole("button", { name: "Install v0.94" })).toBeInTheDocument();
  });

  it("installs a release after confirmation", async () => {
    const { user, socket } = renderReleases();
    await user.click(await screen.findByRole("button", { name: "Install v0.96" }));
    const dialog = within(screen.getByRole("dialog", { name: "Install v0.96?" }));
    expect(dialog.queryByText(/the server will refuse/)).not.toBeInTheDocument();

    await user.click(dialog.getByRole("button", { name: "Install v0.96" }));
    expect(socket.actions()).toEqual([["update", "v0.96", ""]]);
    expect(screen.getByText(/Installing v0\.96/)).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("warns in the dialog while a job runs, and cancels cleanly", async () => {
    const { user, socket } = renderReleases(
      {},
      { machine: { status: { uploadFlag: 1, positioningMode: 0, currentTool: 0 } } },
    );
    await user.click(await screen.findByRole("button", { name: "Install v0.94" }));
    expect(screen.getByRole("dialog")).toHaveTextContent("A job is running; the server will refuse to update");
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(socket.actions()).toEqual([]);
  });

  it("explains an empty list", async () => {
    renderReleases({ "/api/system/releases": releases({ releases: [], latest: null }) });
    expect(await screen.findByText(/No release information yet/)).toBeInTheDocument();
  });

  it("retries a failed load", async () => {
    let down = true;
    const respond = () => (down ? jsonResponse({}, 503) : jsonResponse(releases()));
    const { user } = renderReleases({ "/api/system/releases": respond });
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Could not load releases");
    down = false;
    await user.click(within(alert).getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("Installed version: v0.95")).toBeInTheDocument();
  });
});
