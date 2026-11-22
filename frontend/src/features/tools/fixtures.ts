import type { Tool } from "../../protocol";

export const libraryTool = (number: number, description: string, over: Partial<Tool> = {}): Tool => ({
  number,
  description,
  diameter: null,
  units: null,
  flutes: null,
  notes: "",
  ...over,
});
