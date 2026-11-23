import type { MacroCommand } from "../../protocol";
import type { MachineState } from "../../state/machine";

export const MACRO_COMMANDS: MacroCommand[] = ["macro1", "macro2", "macro3", "macro4"];

export interface MacroEntry {
  command: MacroCommand;
  title: string;
  /** No gcode to run: blank, or a macro the server does not list. */
  empty: boolean;
}

/** All four macros, each with the title to show; a blank title falls back to "Macro N". */
export const macroEntries = (macros: MachineState["macros"]): MacroEntry[] =>
  MACRO_COMMANDS.map((command) => {
    const macro = macros.find((listed) => listed.command === command);
    return { command, title: macro?.title.trim() || `Macro ${command.slice(-1)}`, empty: macro?.empty ?? true };
  });

export const macroEntry = (macros: MachineState["macros"], command: MacroCommand) =>
  macroEntries(macros).find((entry) => entry.command === command) as MacroEntry;
