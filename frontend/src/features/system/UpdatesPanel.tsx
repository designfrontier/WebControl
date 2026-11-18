import type { Updates } from "../../protocol";
import { LazyMarkdown } from "./LazyMarkdown";
import { LastRun, UpdateButtons, UpdateRun } from "./UpdateControls";
import { errorText } from "./useLoad";
import { explanation, headline, safeUrl, updateCommand, when } from "./updates";
import { useUpdatesFeed } from "./useUpdatesFeed";

const LatestRelease = ({ latest }: { latest: NonNullable<Updates["latest"]> }) => (
  <article className="release" aria-label="Latest release">
    <div className="release-head">
      <h3>{latest.name === latest.tag ? latest.tag : `${latest.name} (${latest.tag})`}</h3>
      {safeUrl(latest.url) && (
        <a href={safeUrl(latest.url)} target="_blank" rel="noopener noreferrer">
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
    <h3>{updates.canUpdate ? "Update from a terminal instead" : "How to update"}</h3>
    {updates.canUpdate ? (
      <p>Running webcontrol-update on the Pi does what the buttons above do, and takes options they do not.</p>
    ) : (
      <p>
        This install cannot update from this page yet. Update once on the Pi as below; that adds what the page needs,
        and the Update and Roll back buttons appear here.
      </p>
    )}
    <ol className="update-steps">
      <li>Finish or stop any job. The updater refuses while one is running or paused.</li>
      <li>
        Open a terminal on the Pi as the user WebControl runs as (usually <code>pi</code>):{" "}
        <code>ssh pi@{window.location.hostname}</code>
      </li>
      <li>
        {updates.status === "ahead" && updates.latest
          ? `Only to put ${updates.latest.tag} in place of this build, run`
          : "Run"}{" "}
        <code>{updateCommand(updates)}</code>. It checks the download against <code>SHA256SUMS</code>, backs up this
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

interface ReportProps {
  updates: Updates;
  reload: () => void;
  onStarted: (next: Updates) => void;
  reconnecting: boolean;
  watched: boolean;
}

const Report = ({ updates, reload, onStarted, reconnecting, watched }: ReportProps) => {
  const { text, tone } = headline(updates);
  const { installed, latest, error, backup, checkedAt, repo, progress } = updates;
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
      {progress?.running && <UpdateRun progress={progress} reconnecting={reconnecting} />}
      {!progress?.running && updates.canUpdate && <UpdateButtons updates={updates} onStarted={onStarted} />}
      {progress && !progress.running && <LastRun progress={progress} watched={watched} />}
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

/** What is installed, the latest published release, and updating or rolling back, here or on the Pi. */
export const UpdatesPanel = () => {
  const { loaded, data, reconnecting, watched, show, reload } = useUpdatesFeed();
  return (
    <section aria-label="Updates" className="system-panel">
      {!data && loaded.status === "loading" && <p role="status">Checking for updates…</p>}
      {!data && loaded.status === "error" && (
        <div className="system-error" role="alert">
          <p>Could not load update information: {errorText(loaded.error)}</p>
          <button type="button" onClick={loaded.reload}>
            Retry
          </button>
        </div>
      )}
      {data && <Report updates={data} reload={reload} onStarted={show} reconnecting={reconnecting} watched={watched} />}
    </section>
  );
};
