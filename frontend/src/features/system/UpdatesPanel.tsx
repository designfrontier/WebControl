import type { Updates } from "../../protocol";
import { getUpdates } from "./api";
import { LazyMarkdown } from "./LazyMarkdown";
import { errorText, useLoad } from "./useLoad";
import { explanation, headline, safeUrl, updateCommand, when } from "./updates";

const LatestRelease = ({ latest }: { latest: NonNullable<Updates["latest"]> }) => (
  <article className="release" aria-label="Latest release">
    <div className="release-head">
      <h3>{latest.name === latest.tag ? latest.tag : `${latest.name} (${latest.tag})`}</h3>
      {safeUrl(latest.url) && (
        <a href={safeUrl(latest.url)} target="_blank" rel="noreferrer">
          View on GitHub
        </a>
      )}
    </div>
    {latest.publishedAt && <p className="muted">Published {when(latest.publishedAt)}</p>}
    {latest.notes ? <LazyMarkdown>{latest.notes}</LazyMarkdown> : <p className="muted">No release notes.</p>}
  </article>
);

const HowToUpdate = ({ updates }: { updates: Updates }) => (
  <section aria-label="How to update" className="system-card">
    <h3>How to update</h3>
    <p>
      Updates are installed on the Pi, not from this page: the updater needs the <code>sudo</code> password and restarts
      WebControl.
    </p>
    <ol className="update-steps">
      <li>Finish or stop any job. The updater refuses while one is running or paused.</li>
      <li>
        Open a terminal on the Pi as the user WebControl runs as (usually <code>pi</code>):{" "}
        <code>ssh pi@{window.location.hostname}</code>
      </li>
      <li>
        Run <code>{updateCommand(updates)}</code>. It checks the download against <code>SHA256SUMS</code>, backs up this
        install, restarts WebControl and rolls back by itself if the new version does not come up.
      </li>
      <li>Reload this page when it reports the update.</li>
    </ol>
    <p>
      <code>webcontrol-update --status</code> shows the same from the Pi, and <code>webcontrol-update --rollback</code>{" "}
      puts the backup back.
    </p>
    <p className="muted">
      Only published releases count: a draft release on GitHub is not visible here or to webcontrol-update until it is
      published.
    </p>
  </section>
);

const Report = ({ updates, reload }: { updates: Updates; reload: () => void }) => {
  const { text, tone } = headline(updates);
  const { installed, latest, error, backup, checkedAt, repo } = updates;
  return (
    <>
      <div className="release-head">
        <h3 className="chip" data-tone={tone}>
          {text}
        </h3>
        {error && (
          <button type="button" onClick={reload}>
            Check again
          </button>
        )}
      </div>
      {error && (
        <p className="system-error" role="alert">
          {latest ? "Couldn't reach GitHub just now; showing the release found earlier. " : ""}
          {error}
        </p>
      )}
      {latest && <p>{explanation(updates)}</p>}
      <dl className="facts">
        <dt>Installed</dt>
        <dd>
          webcontrol-rs {installed.version} (commit {installed.commit})
        </dd>
        <dt>Backup</dt>
        <dd>{backup ? `${backup.version} from ${when(backup.date)}` : "None yet"}</dd>
        <dt>Releases from</dt>
        <dd>{repo}</dd>
        {checkedAt !== null && (
          <>
            <dt>Checked</dt>
            <dd>{when(checkedAt, { dateStyle: "medium", timeStyle: "short" })}, at most every 10 minutes</dd>
          </>
        )}
      </dl>
      {latest && <LatestRelease latest={latest} />}
      <HowToUpdate updates={updates} />
    </>
  );
};

/** What is installed, the latest published release, and how to update with webcontrol-update on the Pi. */
export const UpdatesPanel = () => {
  const updates = useLoad(getUpdates);
  return (
    <section aria-label="Updates" className="system-panel">
      {updates.status === "loading" && <p role="status">Checking for updates…</p>}
      {updates.status === "error" && (
        <div className="system-error" role="alert">
          <p>Could not load update information: {errorText(updates.error)}</p>
          <button type="button" onClick={updates.reload}>
            Retry
          </button>
        </div>
      )}
      {updates.status === "ready" && <Report updates={updates.data} reload={updates.reload} />}
    </section>
  );
};
