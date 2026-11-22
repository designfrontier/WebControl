import type { ReactNode } from "react";
import { onlyAsksFor, toolStatus } from "./describe";
import { useFileTools, useToolLibrary } from "./hooks";
import { ToolStatusView } from "./ToolStatusView";

interface Props {
  tool: number;
  /** The firmware's words, folded into the tool's line when they only ask for this tool. */
  message: string;
  messageId: string;
  children?: ReactNode;
}

/** The tool-change prompt's text: which bit to insert, from the loaded file and the tool library. */
export const ToolChangeInfo = ({ tool, message, messageId, children }: Props) => {
  const fileTools = useFileTools();
  const { load, replace } = useToolLibrary();
  const folded = onlyAsksFor(message, tool);
  const firmware = !folded && (
    <p id={messageId} className="server-dialog-message">
      {message}
      {children}
    </p>
  );
  if (load.status === "loading")
    return (
      firmware || (
        <p id={messageId} className="tool-headline">
          Insert Tool {tool}
          {children}
        </p>
      )
    );
  const library = load.status === "ready" ? load.tools : [];
  return (
    <>
      {firmware}
      <ToolStatusView
        status={toolStatus(tool, library, fileTools)}
        onSaved={replace}
        {...(folded ? { lead: "Insert ", headlineId: messageId, children } : {})}
      />
    </>
  );
};
