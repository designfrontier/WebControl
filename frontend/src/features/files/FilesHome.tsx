import { Link } from "react-router";
import { useClient, useServerMessage } from "../../app/context";
import type { LoadedGcode } from "../../protocol";
import { getLoadedGcode } from "./api";
import { useResource } from "./hooks";
import { describeLocation } from "./library";
import { Loaded } from "./ui";

const LoadedSummary = ({ loaded }: { loaded: LoadedGcode }) =>
  loaded.lineCount === 0 ? (
    <p>No gcode is loaded.</p>
  ) : (
    <p>
      {loaded.name ? <strong>{describeLocation(loaded.directory, loaded.name)}</strong> : "Edited gcode"}
      {" — "}
      {loaded.lineCount.toLocaleString("en-US")} {loaded.lineCount === 1 ? "line" : "lines"}
    </p>
  );

export const FilesHome = () => {
  const client = useClient();
  const { resource, reload } = useResource(getLoadedGcode);
  // The spinner announces every gcode push; the gzip payload itself arrives as binary, which the schema rejects.
  useServerMessage("showFPSpinner", reload);
  useServerMessage("gcodeUpdateCompressed", reload);

  return (
    <>
      <section className="panel" aria-labelledby="files-loaded">
        <h2 id="files-loaded">Loaded gcode</h2>
        <Loaded resource={resource} what="the loaded gcode">
          {(loaded) => <LoadedSummary loaded={loaded} />}
        </Loaded>
        <div className="actions">
          <Link className="button" to="/files/open">
            Open…
          </Link>
          <Link className="button" to="/files/upload">
            Upload…
          </Link>
          <Link className="button" to="/files/save">
            Save as…
          </Link>
          <Link className="button" to="/files/edit">
            View or edit…
          </Link>
          <Link className="button" to="/files/send">
            Send gcode…
          </Link>
          <button type="button" onClick={() => client.sendAction("clearGCode")}>
            Clear gcode
          </button>
        </div>
      </section>
      <section className="panel" aria-labelledby="files-backup">
        <h2 id="files-backup">Backup and import</h2>
        <div className="actions">
          <a className="button" href="/downloadDiagnostics" download>
            Download diagnostics
          </a>
          <a className="button" href="/backupWebControl" download>
            Back up WebControl
          </a>
          <Link className="button" to="/files/import/restore">
            Restore WebControl…
          </Link>
          <Link className="button" to="/files/import/groundcontrol">
            Import groundcontrol.ini…
          </Link>
          <Link className="button" to="/files/import/webcontrol">
            Import webcontrol.json…
          </Link>
        </div>
      </section>
    </>
  );
};
