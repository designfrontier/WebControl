import { useState } from "react";
import type { ReactNode } from "react";
import type { Tool } from "../../protocol";
import { toolHeadline, toolNote } from "./describe";
import type { ToolStatus } from "./describe";
import { ToolForm } from "./ToolForm";

interface Props {
  status: ToolStatus;
  onSaved: (tools: Tool[]) => void;
  /** Words before the headline, such as the prompt's "Insert ". */
  lead?: string;
  headlineId?: string;
  children?: ReactNode;
}

/** One tool's headline, where its description came from, and the one-click add or update the file offers. */
export const ToolStatusView = ({ status, onSaved, lead = "", headlineId, children }: Props) => {
  const [editing, setEditing] = useState(false);
  const note = toolNote(status);
  const offer =
    status.kind === "fromFile" ? "Add to tool library" : status.kind === "differs" ? "Update tool library" : null;
  return (
    <div className="tool-status" data-kind={status.kind}>
      <p id={headlineId} className="tool-headline">
        {lead}
        {toolHeadline(status)}
        {children}
      </p>
      {note && <p className="tool-note muted">{note}</p>}
      {offer && !editing && (
        <button type="button" className="tool-offer" onClick={() => setEditing(true)}>
          {offer}
        </button>
      )}
      {editing && (status.kind === "fromFile" || status.kind === "differs") && (
        <ToolForm
          initial={status.suggestion}
          replaces={status.kind === "differs" ? status.number : undefined}
          saveLabel={offer ?? undefined}
          onSaved={(tools) => {
            setEditing(false);
            onSaved(tools);
          }}
          onCancel={() => setEditing(false)}
        />
      )}
    </div>
  );
};
