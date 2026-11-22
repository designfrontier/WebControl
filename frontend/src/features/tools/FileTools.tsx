import { toolStatus } from "./describe";
import { useFileTools, useToolLibrary } from "./hooks";
import { ToolStatusView } from "./ToolStatusView";

/** The tools the loaded file uses, folded to one line until opened. */
export const FileTools = () => {
  const fileTools = useFileTools();
  const { load, replace } = useToolLibrary(fileTools.length > 0);
  if (fileTools.length === 0) return null;
  const library = load.status === "ready" ? load.tools : [];
  const statuses = fileTools.map(({ number }) => toolStatus(number, library, fileTools));
  const offers = statuses.filter(({ kind }) => kind === "fromFile" || kind === "differs").length;
  return (
    <details className="file-tools">
      <summary>
        Tools: {fileTools.map(({ number }) => `T${number}`).join(", ")}
        {load.status === "ready" && offers > 0 && <span className="muted"> ({offers} to review)</span>}
      </summary>
      <ul>
        {statuses.map((status) => (
          <li key={status.number}>
            <ToolStatusView status={status} onSaved={replace} />
          </li>
        ))}
      </ul>
    </details>
  );
};
