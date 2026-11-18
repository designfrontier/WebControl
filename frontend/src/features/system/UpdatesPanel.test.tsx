import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { jsonResponse } from "../../test/fakeFetch";
import { renderApp } from "../../test/render";
import { latestRelease, systemFetch, updates } from "../../test/systemFixtures";

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
    expect(steps().getByText(/not from this page/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /install/i })).not.toBeInTheDocument();
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
});
