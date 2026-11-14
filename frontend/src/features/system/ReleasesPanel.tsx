import { useState } from "react";
import { useClient, useMachine } from "../../app/context";
import type { Releases } from "../../protocol";
import { selectRunState } from "../../state/machine";
import { getReleases } from "./api";
import { ConfirmDialog } from "./Dialog";
import { LazyMarkdown as MarkdownView } from "./LazyMarkdown";
import { errorText, useLoad } from "./useLoad";

type Release = Releases["releases"][number];

interface ItemProps {
  release: Release;
  releases: Releases;
  busy: boolean;
  onPick: () => void;
}

const ReleaseItem = ({ release, releases, busy, onPick }: ItemProps) => {
  const current = release.tag === releases.current;
  const latest = release.tag === releases.latest;
  return (
    <li className="release">
      <div className="release-head">
        <h3>{release.title === release.tag ? release.tag : `${release.title} (${release.tag})`}</h3>
        {current && <span className="chip">Installed</span>}
        {latest && !current && (
          <span className="chip" data-tone="ok">
            Latest
          </span>
        )}
        {!current && (
          <button type="button" className={latest ? "button-primary" : undefined} disabled={busy} onClick={onPick}>
            Install {release.tag}
          </button>
        )}
      </div>
      {release.body && <MarkdownView>{release.body}</MarkdownView>}
    </li>
  );
};

/** The classic "Update Manager": GitHub releases with an install button each. */
export const ReleasesPanel = ({ updateAvailable }: { updateAvailable: boolean }) => {
  const client = useClient();
  const runState = useMachine(selectRunState);
  const releases = useLoad(getReleases);
  const busy = runState === "running" || runState === "paused";
  const [picked, setPicked] = useState<string | null>(null);
  const [notice, setNotice] = useState("");

  const install = (tag: string) => {
    client.sendAction("update", tag);
    setNotice(`Installing ${tag}. WebControl restarts when it is done; reload this page then.`);
    setPicked(null);
  };

  return (
    <section aria-labelledby="releases-title" className="system-panel">
      <h2 id="releases-title">Updates</h2>
      {updateAvailable && (
        <p className="chip" data-tone="ok">
          An update is available
        </p>
      )}
      <p className="system-status" role="status">
        {notice}
      </p>
      {busy && (
        <p className="system-note">
          A job is {runState}. Installing stays disabled until it is stopped, as the server would refuse.
        </p>
      )}
      {releases.status === "loading" && <p role="status">Loading releases…</p>}
      {releases.status === "error" && (
        <div className="system-error" role="alert">
          <p>Could not load releases: {errorText(releases.error)}</p>
          <button type="button" onClick={releases.reload}>
            Retry
          </button>
        </div>
      )}
      {releases.status === "ready" && (
        <>
          <p>Installed version: {releases.data.current}</p>
          {releases.data.releases.length === 0 ? (
            <p className="muted">
              No release information yet. WebControl checks GitHub for releases about once an hour when it is online.
            </p>
          ) : (
            <ul className="release-list">
              {releases.data.releases.map((release) => (
                <ReleaseItem
                  key={release.tag}
                  release={release}
                  releases={releases.data}
                  busy={busy}
                  onPick={() => setPicked(release.tag)}
                />
              ))}
            </ul>
          )}
        </>
      )}
      {picked && (
        <ConfirmDialog
          title={`Install ${picked}?`}
          confirmLabel={`Install ${picked}`}
          onConfirm={() => install(picked)}
          onCancel={() => setPicked(null)}
        >
          <p>
            WebControl downloads {picked}, installs it and restarts. Every connected browser loses its connection
            meanwhile.
          </p>
          {busy && (
            <p className="system-error">A job is {runState}; the server will refuse to update until it is stopped.</p>
          )}
        </ConfirmDialog>
      )}
    </section>
  );
};
