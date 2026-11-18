import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { jsonResponse, startupState } from "../../test/fakeFetch";
import { renderApp } from "../../test/render";
import { latestRelease, systemFetch, updateProgress, updates } from "../../test/systemFixtures";

vi.mock("./updateProgress", async (original) => ({ ...(await original<object>()), POLL_MS: 10 }));

const renderUpdates = (routes = {}) => renderApp("/updates", { fetch: systemFetch(routes) });

const steps = () => within(screen.getByRole("region", { name: "How to update" }));

describe("UpdatesPanel", () => {
  it("shows an available update with its notes, the backup and the command to run on the Pi", async () => {
    const { socket } = renderUpdates();
    expect(await screen.findByRole("heading", { name: "Update available: v0.4.0" })).toBeInTheDocument();
    expect(screen.getByText("webcontrol-rs 0.3.0 (commit dc9bdca)")).toBeInTheDocument();
    expect(screen.getByText(/^0\.2\.0 from .*2026/)).toBeInTheDocument();
    expect(screen.getByText(/v0\.4\.0 is published; this Pi runs 0\.3\.0/)).toBeInTheDocument();

    const release = within(screen.getByRole("article", { name: "Latest release" }));
    expect(release.getByRole("heading", { name: "webcontrol-rs 0.4.0 (v0.4.0)" })).toBeInTheDocument();
    expect(release.getByRole("link", { name: "View on GitHub" })).toHaveAttribute("target", "_blank");
    expect(await release.findByRole("heading", { name: "Faster previews" })).toBeInTheDocument();
    expect(release.getByRole("link", { name: "#12" })).toHaveAttribute("href", "https://github.com/x/y/issues/12");
    expect(document.querySelector(".markdown img")).toBeNull();

    expect(steps().getByText("webcontrol-update")).toBeInTheDocument();
    expect(steps().getByText(`ssh pi@${window.location.hostname}`)).toBeInTheDocument();
    expect(steps().getByText(/a draft release on GitHub is not visible/)).toBeInTheDocument();
    expect(steps().getByRole("heading", { name: "Update from a terminal instead" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Update to v0.4.0" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Roll back to 0.2.0" })).toBeEnabled();
    expect(socket.actions()).toEqual([]);
  });

  it("says when the installed release is the latest, with no backup yet", async () => {
    renderUpdates({
      "/api/system/updates": updates({
        status: "upToDate",
        installed: { version: "0.4.0", commit: "1234567" },
        backup: null,
        latest: latestRelease({ name: "v0.4.0", notes: "" }),
      }),
    });
    expect(await screen.findByRole("heading", { name: "Up to date" })).toBeInTheDocument();
    expect(screen.getByText("None yet")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "v0.4.0" })).toBeInTheDocument();
    expect(screen.getByText("No release notes.")).toBeInTheDocument();
  });

  it("offers --force for a dev build of the released version, and --repo for a fork", async () => {
    renderUpdates({
      "/api/system/updates": updates({
        status: "devBuild",
        installed: { version: "0.4.0", commit: "abcdef1" },
        repo: "someone/WebControl",
      }),
    });
    expect(await screen.findByRole("heading", { name: "Development build" })).toBeInTheDocument();
    expect(
      screen.getByText(/development build of 0\.4\.0 \(commit abcdef1\), not the v0\.4\.0 release \(commit 1234567\)/),
    ).toBeInTheDocument();
    expect(steps().getByText("webcontrol-update --repo someone/WebControl --force")).toBeInTheDocument();
  });

  it("says a build ahead of the latest release has nothing to update to", async () => {
    renderUpdates({
      "/api/system/updates": updates({ status: "ahead", installed: { version: "0.5.0", commit: "x" } }),
    });
    expect(await screen.findByRole("heading", { name: "Newer than the latest release" })).toBeInTheDocument();
    expect(screen.getByText(/There is nothing to update to/)).toBeInTheDocument();
  });

  it("explains an offline Pi and checks again", async () => {
    let offline = true;
    const respond = () =>
      jsonResponse(
        offline
          ? updates({ status: "unknown", latest: null, error: "Couldn't reach GitHub: dns failed", checkedAt: null })
          : updates(),
      );
    const { user } = renderUpdates({ "/api/system/updates": respond });
    expect(await screen.findByRole("heading", { name: "Couldn't reach GitHub" })).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("Couldn't reach GitHub: dns failed");
    expect(screen.queryByRole("article")).not.toBeInTheDocument();
    expect(screen.queryByText("Checked")).not.toBeInTheDocument();
    expect(steps().getByText("webcontrol-update")).toBeInTheDocument();

    offline = false;
    await user.click(screen.getByRole("button", { name: "Check again" }));
    expect(await screen.findByRole("heading", { name: "Update available: v0.4.0" })).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("keeps the release found earlier when GitHub stops answering", async () => {
    renderUpdates({ "/api/system/updates": updates({ error: "GitHub answered HTTP 502" }) });
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Couldn't reach GitHub just now; showing the release found earlier. GitHub answered HTTP 502",
    );
    expect(screen.getByRole("heading", { name: "Update available: v0.4.0" })).toBeInTheDocument();
  });

  it("does not compare versions it cannot read, and links nowhere but https", async () => {
    renderUpdates({
      "/api/system/updates": updates({
        status: "unknown",
        installed: { version: "dev", commit: "unknown" },
        latest: latestRelease({ url: "javascript:alert(1)", publishedAt: null }),
      }),
    });
    expect(
      await screen.findByRole("heading", { name: "Can't tell whether an update is available" }),
    ).toBeInTheDocument();
    expect(screen.getByText(/The installed version \(dev\) or the release tag \(v0\.4\.0\)/)).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "View on GitHub" })).not.toBeInTheDocument();
    expect(screen.queryByText(/^Published/)).not.toBeInTheDocument();
  });

  it("retries when the server cannot be asked", async () => {
    let down = true;
    const respond = () => (down ? jsonResponse({}, 503) : jsonResponse(updates()));
    const { user } = renderUpdates({ "/api/system/updates": respond });
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Could not load update information");
    down = false;
    await user.click(within(alert).getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("heading", { name: "Update available: v0.4.0" })).toBeInTheDocument();
  });

  it("tells an install without the updater's units to update once on the Pi, and offers no buttons", async () => {
    renderUpdates({ "/api/system/updates": updates({ canUpdate: false }) });
    expect(await screen.findByRole("heading", { name: "How to update" })).toBeInTheDocument();
    expect(steps().getByText(/cannot update from this page yet/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Update to|Roll back/ })).not.toBeInTheDocument();
  });

  it("offers no buttons with nothing newer and no backup", async () => {
    renderUpdates({ "/api/system/updates": updates({ status: "upToDate", backup: null }) });
    expect(await screen.findByRole("heading", { name: "Up to date" })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Update or roll back" })).not.toBeInTheDocument();
  });

  it("confirms with the release notes, starts the update, follows it through the restart and shows the result", async () => {
    let polls = 0;
    let done = false;
    const progressAt = () => {
      polls += 1;
      if (polls === 1) return jsonResponse(updates({ progress: updateProgress({ phase: "installing" }) }));
      if (polls === 2) return jsonResponse({}, 502);
      if (!done) return jsonResponse(updates({ progress: updateProgress({ phase: "healthCheck" }) }));
      const updated = updateProgress({ phase: "finished", running: false, result: "updated", finishedAt: 1 });
      return jsonResponse(
        updates({ status: "upToDate", installed: { version: "0.4.0", commit: "1234567" }, progress: updated }),
      );
    };
    let started = false;
    const fetch = systemFetch({
      "/api/system/updates": () => (started ? progressAt() : jsonResponse(updates())),
      "POST /api/system/updates/install": () => {
        started = true;
        return jsonResponse(updates({ progress: updateProgress({ phase: "starting", log: [] }) }), 202);
      },
    });
    const { user } = renderApp("/updates", { fetch });
    await user.click(await screen.findByRole("button", { name: "Update to v0.4.0" }));
    const confirm = screen.getByRole("dialog", { name: "Update to v0.4.0?" });
    expect(within(confirm).getByText(/backs up this install \(0\.3\.0\)/)).toBeInTheDocument();
    expect(within(confirm).getAllByText(/reconnects by itself/).length).toBeGreaterThan(0);
    expect(await within(confirm).findByRole("heading", { name: "Faster previews" })).toBeInTheDocument();
    await user.click(within(confirm).getByRole("button", { name: "Update now" }));

    const run = await screen.findByRole("region", { name: "Update progress" });
    expect(within(run).getByRole("heading", { name: "Updating to v0.4.0" })).toBeInTheDocument();
    expect(fetch).toHaveBeenCalledWith("/api/system/updates/install", expect.objectContaining({ method: "POST" }));
    expect(await within(run).findByText("WebControl is restarting; reconnecting…")).toBeInTheDocument();
    const current = within(run).getByText("Checking the new version answers").closest("li");
    await waitFor(() => expect(current).toHaveAttribute("aria-current", "step"));
    expect(within(run).getByText("Checksum OK").closest("li")).toHaveAttribute("data-state", "done");
    done = true;

    const last = await screen.findByRole("region", { name: "Last update" });
    expect(within(last).getByRole("status")).toHaveTextContent("Updated from 0.3.0 to 0.4.0.");
    expect(within(last).getByRole("button", { name: "Reload the page" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Up to date" })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Update progress" })).not.toBeInTheDocument();
  });

  it("confirms a rollback, warns its settings come back too, and shows a refusal", async () => {
    const fetch = systemFetch({
      "POST /api/system/updates/rollback": () =>
        jsonResponse({ error: "An update or rollback is already running." }, 409),
    });
    const { user } = renderApp("/updates", { fetch });
    await user.click(await screen.findByRole("button", { name: "Roll back to 0.2.0" }));
    const confirm = screen.getByRole("dialog", { name: "Roll back to 0.2.0?" });
    expect(within(confirm).getByText(/settings changed since then are lost/)).toBeInTheDocument();
    await user.click(within(confirm).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog", { name: "Roll back to 0.2.0?" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Roll back to 0.2.0" }));
    await user.click(screen.getByRole("button", { name: "Roll back now" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("An update or rollback is already running.");
    expect(screen.getByRole("button", { name: "Roll back to 0.2.0" })).toBeEnabled();
  });

  it("turns the buttons off with the reason while a job runs or the server says the machine is busy", async () => {
    renderUpdates({ "GET /api/state": startupState({ jobInProgress: true }) });
    const update = await screen.findByRole("button", { name: "Update to v0.4.0" });
    expect(update).toBeDisabled();
    expect(update).toHaveAccessibleDescription("Not now: a job is running or paused. Finish or stop it first.");
    expect(screen.getByRole("button", { name: "Roll back to 0.2.0" })).toBeDisabled();
  });

  it("uses the server's busy reason, such as an optical scan", async () => {
    const busy = "Not now: an optical calibration scan is running. Finish or stop it first.";
    renderUpdates({ "/api/system/updates": updates({ busy }) });
    expect(await screen.findByRole("button", { name: "Update to v0.4.0" })).toHaveAccessibleDescription(busy);
  });

  it("opens on a run already going, as after a reload mid-update, and on an automatic rollback's result", async () => {
    let finished = false;
    const running = updateProgress({
      phase: "rollingBack",
      log: ["UPDATE FAILED: rolling back to webcontrol-rs 0.3.0."],
    });
    const rolledBack = updateProgress({
      phase: "finished",
      running: false,
      result: "rolledBack",
      log: ["ROLLED BACK"],
    });
    renderUpdates({
      "/api/system/updates": () => jsonResponse(updates({ progress: finished ? rolledBack : running })),
    });
    const run = await screen.findByRole("region", { name: "Update progress" });
    expect(within(run).getByText("Restoring the backup").closest("li")).toHaveAttribute("aria-current", "step");
    finished = true;
    const last = await screen.findByRole("region", { name: "Last update" });
    expect(within(last).getByRole("status")).toHaveTextContent("The update to 0.4.0 failed, so 0.3.0 was put back.");
    expect(within(last).getByText("ROLLED BACK")).toBeInTheDocument();
  });

  it("shows an earlier finished run without claiming this page is stale", async () => {
    const failed = updateProgress({ action: "rollback", phase: "finished", running: false, result: "rollbackFailed" });
    renderUpdates({ "/api/system/updates": updates({ progress: failed }) });
    const last = await screen.findByRole("region", { name: "Last update" });
    expect(within(last).getByRole("heading", { name: "Last rollback from this page" })).toBeInTheDocument();
    expect(within(last).getByText(/The rollback failed too/)).toBeInTheDocument();
    expect(within(last).queryByRole("button", { name: "Reload the page" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Update to v0.4.0" })).toBeInTheDocument();
  });
});
