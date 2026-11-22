import type { FileTool, Tool } from "../../protocol";

export const libraryTool = (number: number, description: string, over: Partial<Tool> = {}): Tool => ({
  number,
  description,
  diameter: null,
  units: null,
  flutes: null,
  notes: "",
  ...over,
});

export const fileTool = (number: number, description: string | null, over: Partial<FileTool> = {}): FileTool => ({
  number,
  description,
  diameter: null,
  units: null,
  flutes: null,
  ...over,
});
