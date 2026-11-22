import { TOOL_NUMBER_MAX } from "../../protocol";
import type { ToolInput, ToolUnits } from "../../protocol";

/** The tool form's fields as typed. */
export interface Draft {
  number: string;
  description: string;
  diameter: string;
  units: ToolUnits;
  flutes: string;
  notes: string;
}

export const draftOf = (tool: ToolInput): Draft => ({
  number: String(tool.number),
  description: tool.description,
  diameter: tool.diameter == null ? "" : String(tool.diameter),
  units: tool.units ?? "mm",
  flutes: tool.flutes == null ? "" : String(tool.flutes),
  notes: tool.notes ?? "",
});

const optionalNumber = (text: string) => (text.trim() === "" ? null : Number(text));

/** The tool as the server takes it, or what to fix first. */
export const toolInput = (draft: Draft): ToolInput | string => {
  const number = Number(draft.number);
  if (!Number.isInteger(number) || number < 1 || number > TOOL_NUMBER_MAX) return "The tool number must be 1 to 99999.";
  if (draft.description.trim() === "") return 'Describe the tool, e.g. 1/4" 2 flute end mill.';
  const diameter = optionalNumber(draft.diameter);
  if (diameter !== null && !(diameter > 0)) return "The diameter must be a number above 0.";
  const flutes = optionalNumber(draft.flutes);
  if (flutes !== null && !Number.isInteger(flutes)) return "Flutes must be a whole number.";
  return {
    number,
    description: draft.description.trim(),
    diameter,
    units: diameter === null ? null : draft.units,
    flutes,
    notes: draft.notes.trim(),
  };
};
