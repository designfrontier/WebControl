import { Fragment } from "react";
import { useMachine } from "../../app/context";
import "./loadedFile.css";

/** "jobs/leg.nc" as its folder and file name; a file at the library root has no folder. */
const splitGcodePath = (path: string) => {
  const at = path.lastIndexOf("/");
  return { folder: path.slice(0, Math.max(at, 0)), name: path.slice(at + 1) };
};

/** A long name wraps after its dots, dashes and underscores rather than mid-word. */
const breakable = (name: string) =>
  // No lookbehind: Safari before 16.4 refuses to parse the whole bundle over one.
  (name.match(/[^._-]+[._-]?|[._-]/g) ?? []).map((part, at) => (
    <Fragment key={at}>
      {at > 0 ? <wbr /> : null}
      {part}
    </Fragment>
  ));

/** The loaded gcode file: its name, then its folder, over at most two lines with an ellipsis past them. */
export const LoadedFile = ({ className }: { className: string }) => {
  const file = useMachine((s) => s.gcodeFile);
  const { folder, name } = splitGcodePath(file ?? "");
  return (
    <p className={`loaded-file ${className}`} title={file ?? undefined} data-loaded={file !== null}>
      <span className="loaded-file-label">File:</span>{" "}
      <span className="loaded-file-name">{file === null ? "No file loaded" : breakable(name)}</span>
      {folder ? <span className="loaded-file-folder"> in {folder}</span> : null}
    </p>
  );
};
