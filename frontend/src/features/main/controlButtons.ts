import type { ControlButton, MacroCommand, ParkPosition } from "../../protocol";
import type { MachineState } from "../../state/machine";
import type { ControlAvailability } from "./controls";
import { macroEntries, macroEntry } from "./macros";
import { NO_PARK, parkText } from "./park";

export const CONTROL_BUTTONS: ControlButton[] = [
  "park",
  "zAxis",
  "setHome",
  "setPark",
  "macro1",
  "macro2",
  "macro3",
  "macro4",
];

/** Where each slot sits on the control screen, in slot order. */
export const SLOT_NAMES = [
  "Right column, top",
  "Right column, middle",
  "Right column, bottom",
  "Beside the distance",
] as const;

const FIXED_LABELS = { park: "Park", zAxis: "Z-Axis", setHome: "Set Home Here", setPark: "Set Park Here" } as const;

/** A choice as the settings list it: a macro by its number, and its title when it has one of its own. */
export const choiceLabel = (button: ControlButton, macros: MachineState["macros"]) => {
  if (button in FIXED_LABELS) return FIXED_LABELS[button as keyof typeof FIXED_LABELS];
  const macro = macroEntries(macros).find((entry) => entry.command === button);
  const number = `Macro ${button.slice(-1)}`;
  return macro && macro.title !== number ? `${number}: ${macro.title}` : number;
};

/** The slots after `button` goes into `slot`; a button already in another slot swaps into this one's place. */
export const placeButton = (buttons: ControlButton[], slot: number, button: ControlButton): ControlButton[] => {
  const from = buttons.indexOf(button);
  return buttons.map((current, at) => (at === slot ? button : at === from ? buttons[slot] : current));
};

export interface SlotAction {
  label: string;
  command: "park" | "defineHome" | "definePark" | MacroCommand;
  disabled: boolean;
  /** Why it is disabled, or what it does. */
  title?: string;
}

interface SlotState {
  available: ControlAvailability;
  park: ParkPosition | null;
  macros: MachineState["macros"];
}

/** What a slot's button sends, and when it is disabled; the Z-Axis slot is a link instead. */
export const slotAction = (
  button: Exclude<ControlButton, "zAxis">,
  { available, park, macros }: SlotState,
): SlotAction => {
  const here = (what: string) => `Make the sled's current position the ${what} position`;
  const actions: Record<"park" | "setHome" | "setPark", SlotAction> = {
    park: {
      label: "Park",
      command: "park",
      disabled: !available.defineHome || park === null,
      title: park === null ? NO_PARK : parkText(park),
    },
    setHome: { label: "Set Home Here", command: "defineHome", disabled: !available.defineHome, title: here("home") },
    setPark: { label: "Set Park Here", command: "definePark", disabled: !available.defineHome, title: here("park") },
  };
  if (button in actions) return actions[button as keyof typeof actions];
  const macro = macroEntry(macros, button as MacroCommand);
  const empty = "This macro has no gcode yet: set it in Settings ▸ Maslow.";
  return {
    label: macro.title,
    command: macro.command,
    disabled: !available.manual || macro.empty,
    title: macro.empty ? empty : undefined,
  };
};
