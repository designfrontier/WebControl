import { toolHeadline, toolStatus } from "./describe";
import { useToolLibrary } from "./hooks";
import "./tools.css";

/** Which bit the tool-change prompt asks for, from the tool library. */
export const ToolChangeInfo = ({ tool }: { tool: number }) => {
  const { load } = useToolLibrary();
  if (load.status === "loading") return null;
  const library = load.status === "ready" ? load.tools : [];
  return <p className="tool-headline">{toolHeadline(toolStatus(tool, library))}</p>;
};
