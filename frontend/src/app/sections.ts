import { boardsSection } from "../features/boards";
import { calibrationSection } from "../features/calibration";
import { filesSection } from "../features/files";
import { machineSection } from "../features/machine";
import { mainSection } from "../features/main";
import { settingsSection } from "../features/settings";
import { systemSection } from "../features/system";
import type { Section } from "./section";

/** Nav order. Each feature defines its own Section in features/<name>/index.ts; this list only orders them. */
export const sections: Section[] = [
  mainSection,
  filesSection,
  boardsSection,
  machineSection,
  calibrationSection,
  settingsSection,
  systemSection,
];
