import { useState } from "react";
import { Link } from "react-router";
import { useFetch } from "../../app/context";
import type { Tool } from "../../protocol";
import { deleteTool, refusalText } from "./api";
import { toolDetails } from "./describe";
import { useToolLibrary } from "./hooks";
import { ToolForm } from "./ToolForm";

type Editing =
  { kind: "none" } | { kind: "new" } | { kind: "edit"; number: number } | { kind: "delete"; number: number };

const blankTool = (tools: readonly Tool[]) => ({
  number: tools.reduce((highest, tool) => Math.max(highest, tool.number), 0) + 1,
  description: "",
});

/** Settings ▸ Tools: the library tool-change prompts read; editing it is allowed during a job. */
export const ToolsSection = () => {
  const fetchImpl = useFetch();
  const { load, retry, replace } = useToolLibrary();
  const [editing, setEditing] = useState<Editing>({ kind: "none" });
  const [error, setError] = useState<string | null>(null);
  const done = (tools: Tool[]) => {
    replace(tools);
    setEditing({ kind: "none" });
  };
  const remove = (number: number) => {
    setError(null);
    deleteTool(fetchImpl, number).then(
      (library) => done(library.tools),
      (caught: unknown) => setError(refusalText(caught, `Could not delete tool ${number}. Try again.`)),
    );
  };
  const cancel = () => setEditing({ kind: "none" });

  if (load.status === "loading") return <p className="muted">Loading the tool library…</p>;
  if (load.status === "error")
    return (
      <div role="alert" className="settings-load-error">
        <p>Could not load the tool library.</p>
        <button type="button" onClick={retry}>
          Try again
        </button>
      </div>
    );
  const { tools } = load;
  return (
    <div className="settings-section tools-section">
      <h3>Tools</h3>
      <p className="muted">
        Tool-change prompts name each tool from here. Changes are allowed during a job.{" "}
        <Link to="/help/GettingStarted/toolLibrary.md">How the tool library works</Link>
      </p>
      {editing.kind === "new" ? (
        <ToolForm initial={blankTool(tools)} saveLabel="Add tool" onSaved={done} onCancel={cancel} />
      ) : (
        <button type="button" className="primary tools-add" onClick={() => setEditing({ kind: "new" })}>
          Add tool
        </button>
      )}
      {error && (
        <p role="alert" className="setting-error">
          {error}
        </p>
      )}
      {tools.length === 0 ? (
        <p className="muted">Your tool library is empty. Add the bits your files call for.</p>
      ) : (
        <ul className="tool-list" aria-label="Tool library">
          {tools.map((tool) => (
            <li key={tool.number} className="tool-row">
              {editing.kind === "edit" && editing.number === tool.number ? (
                <ToolForm initial={tool} replaces={tool.number} onSaved={done} onCancel={cancel} />
              ) : (
                <>
                  <span className="tool-badge" aria-hidden="true">
                    T{tool.number}
                  </span>
                  <div className="tool-text">
                    <p className="tool-headline">
                      <span className="visually-hidden">Tool {tool.number}: </span>
                      {tool.description}
                    </p>
                    {toolDetails(tool) && <p className="muted">{toolDetails(tool)}</p>}
                    {tool.notes && <p className="muted tool-notes-text">{tool.notes}</p>}
                  </div>
                  <div className="tool-actions">
                    {editing.kind === "delete" && editing.number === tool.number ? (
                      <>
                        <button type="button" className="danger" onClick={() => remove(tool.number)}>
                          Delete tool {tool.number}
                        </button>
                        <button type="button" onClick={cancel}>
                          Keep
                        </button>
                      </>
                    ) : (
                      <>
                        <button
                          type="button"
                          aria-label={`Edit tool ${tool.number}`}
                          onClick={() => setEditing({ kind: "edit", number: tool.number })}
                        >
                          Edit
                        </button>
                        <button
                          type="button"
                          aria-label={`Delete tool ${tool.number}`}
                          onClick={() => setEditing({ kind: "delete", number: tool.number })}
                        >
                          Delete
                        </button>
                      </>
                    )}
                  </div>
                </>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
};
