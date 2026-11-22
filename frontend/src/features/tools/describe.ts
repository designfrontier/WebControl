import type { Tool } from "../../protocol";

/** What to say about one tool number, given the library. */
export type ToolStatus = { kind: "library"; number: number; description: string } | { kind: "unknown"; number: number };

export const toolStatus = (number: number, library: readonly Tool[]): ToolStatus => {
  const known = library.find((tool) => tool.number === number);
  return known ? { kind: "library", number, description: known.description } : { kind: "unknown", number };
};

export const toolHeadline = (status: ToolStatus) =>
  status.kind === "unknown"
    ? `Tool ${status.number} (not in your tool library)`
    : `Tool ${status.number} — ${status.description}`;

const trimmedNumber = (value: number) => String(Number(value.toFixed(4)));

/** "6.35 mm · 2 flutes", or "" when neither is known. */
export const toolDetails = ({ diameter, units, flutes }: Pick<Tool, "diameter" | "units" | "flutes">) =>
  [
    diameter !== null && units !== null ? `${trimmedNumber(diameter)} ${units}` : null,
    flutes !== null ? `${flutes} flute${flutes === 1 ? "" : "s"}` : null,
  ]
    .filter((part) => part !== null)
    .join(" · ");
