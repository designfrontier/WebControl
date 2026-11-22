import type { ReactNode } from "react";
import { onlyAsksFor, toolHeadline, toolStatus } from "./describe";
import { useToolLibrary } from "./hooks";
import "./tools.css";

interface Props {
  tool: number;
  /** The firmware's words, folded into the tool's line when they only ask for this tool. */
  message: string;
  messageId: string;
  children?: ReactNode;
}

/** The tool-change prompt's text: which bit to insert, from the tool library. */
export const ToolChangeInfo = ({ tool, message, messageId, children }: Props) => {
  const { load } = useToolLibrary();
  const folded = onlyAsksFor(message, tool);
  const library = load.status === "ready" ? load.tools : [];
  const headline = load.status === "loading" ? `Tool ${tool}` : toolHeadline(toolStatus(tool, library));
  return (
    <>
      {!folded && (
        <p id={messageId} className="server-dialog-message">
          {message}
          {children}
        </p>
      )}
      <p className="tool-headline" id={folded ? messageId : undefined}>
        {folded ? `Insert ${headline}` : headline}
        {folded && children}
      </p>
    </>
  );
};
