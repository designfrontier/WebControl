import type { FileTool, Tool, ToolInput } from "../../protocol";

/** What to say about one tool number, given the library and what the loaded file says about it. */
export type ToolStatus =
  | { kind: "library"; number: number; description: string }
  | { kind: "fromFile"; number: number; description: string; suggestion: ToolInput }
  | { kind: "differs"; number: number; description: string; library: string; suggestion: ToolInput }
  | { kind: "unknown"; number: number };

const comparable = (text: string) =>
  text
    .toLowerCase()
    .replace(/\s+/g, " ")
    .replace(/[\s.]+$/, "")
    .trim();

/** The file's description wins over the library's, since the file is what is about to cut. */
export const toolStatus = (number: number, library: readonly Tool[], fileTools: readonly FileTool[]): ToolStatus => {
  const known = library.find((tool) => tool.number === number);
  const fromFile = fileTools.find((tool) => tool.number === number && tool.description);
  if (!fromFile?.description)
    return known ? { kind: "library", number, description: known.description } : { kind: "unknown", number };
  const description = fromFile.description;
  const sized = fromFile.diameter !== null && fromFile.units !== null;
  if (!known) {
    const suggestion = {
      number,
      description,
      diameter: fromFile.diameter,
      units: fromFile.units,
      flutes: fromFile.flutes,
    };
    return { kind: "fromFile", number, description, suggestion };
  }
  if (comparable(known.description) === comparable(description)) return { kind: "library", number, description };
  const suggestion: ToolInput = {
    ...known,
    description,
    diameter: sized ? fromFile.diameter : known.diameter,
    units: sized ? fromFile.units : known.units,
    flutes: fromFile.flutes ?? known.flutes,
  };
  return { kind: "differs", number, description, library: known.description, suggestion };
};

export const toolHeadline = (status: ToolStatus) =>
  status.kind === "unknown"
    ? `Tool ${status.number} (not in your tool library)`
    : `Tool ${status.number} — ${status.description}`;

export const toolNote = (status: ToolStatus) =>
  status.kind === "fromFile"
    ? "From this file; not in your tool library."
    : status.kind === "differs"
      ? `From this file; your library says ${status.library}.`
      : null;

const BARE_INSERT = /^please insert (?:a tool|tool\s*(\d+))\s*[.!]?$/i;

/** Whether the firmware's prompt only asks for tool `tool`, which the tool's own line can then say instead. */
export const onlyAsksFor = (message: string, tool: number) => {
  const found = BARE_INSERT.exec(message.trim());
  return found !== null && (found[1] === undefined || Number(found[1]) === tool);
};

const trimmedNumber = (value: number) => String(Number(value.toFixed(4)));

/** "6.35 mm · 2 flutes", or "" when neither is known. */
export const toolDetails = ({ diameter, units, flutes }: Pick<Tool, "diameter" | "units" | "flutes">) =>
  [
    diameter !== null && units !== null ? `${trimmedNumber(diameter)} ${units}` : null,
    flutes !== null ? `${flutes} flute${flutes === 1 ? "" : "s"}` : null,
  ]
    .filter((part) => part !== null)
    .join(" · ");
