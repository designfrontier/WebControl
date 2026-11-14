import type { ComponentType } from "react";
import type { Params } from "react-router";
import { EditBoard, OpenBoard, SaveBoard, TrimBoard } from "../features/boards";
import { EditGcode, ImportFile, OpenGcode, SaveGcode, SendGcode, UploadGcode, importKinds } from "../features/files";
import {
  FakeServoPage,
  GettingStartedPage,
  QuickConfigurePage,
  ResetChainsPage,
  SprocketsPage,
  ZAxisPage,
  ZLimitsPage,
} from "../features/machine";
import { ZAxisModal } from "../features/main";
import { SettingsPage } from "../features/settings";
import { AboutModal, ActionsModal, HelpPanel, PidTuning, UpdatesModal } from "../features/system";

export interface ModalRoute {
  /** Relative to /next/; the modal renders over the control screen at this address. */
  path: string;
  title: string | ((params: Params) => string);
  Component: ComponentType;
  size?: "small" | "medium" | "large";
}

const importTitle = ({ kind }: Params) =>
  kind && Object.hasOwn(importKinds, kind) ? importKinds[kind as keyof typeof importKinds].title : "Import";

export const modalRoutes: ModalRoute[] = [
  { path: "files/open", title: "Open G-Code", Component: OpenGcode, size: "medium" },
  { path: "files/save", title: "Save G-Code", Component: SaveGcode, size: "medium" },
  { path: "files/upload", title: "Upload G-Code", Component: UploadGcode, size: "medium" },
  { path: "files/edit", title: "View/Edit G-Code", Component: EditGcode, size: "large" },
  { path: "files/send", title: "Send Custom G-Code", Component: SendGcode, size: "large" },
  { path: "files/import/:kind", title: importTitle, Component: ImportFile },
  { path: "actions", title: "Actions", Component: ActionsModal, size: "large" },
  { path: "settings/*", title: "Settings", Component: SettingsPage, size: "large" },
  { path: "boards/open", title: "Open Board", Component: OpenBoard, size: "medium" },
  { path: "boards/save", title: "Save Board", Component: SaveBoard, size: "medium" },
  { path: "boards/edit", title: "Create/Edit Board", Component: EditBoard, size: "medium" },
  { path: "boards/trim", title: "Trim Board", Component: TrimBoard, size: "medium" },
  { path: "zaxis", title: "Z-Axis", Component: ZAxisModal },
  { path: "pid", title: "PID Tuning", Component: PidTuning, size: "large" },
  { path: "help/*", title: "Help", Component: HelpPanel, size: "large" },
  { path: "about", title: "About", Component: AboutModal, size: "medium" },
  { path: "updates", title: "Update", Component: UpdatesModal, size: "medium" },
  { path: "machine/getting-started", title: "Getting Started", Component: GettingStartedPage, size: "large" },
  { path: "machine/quick-configure", title: "Quick Configure", Component: QuickConfigurePage, size: "large" },
  { path: "machine/sprockets", title: "Set Sprockets & Chains", Component: SprocketsPage, size: "medium" },
  { path: "machine/reset-chains", title: "Reset Chains", Component: ResetChainsPage, size: "medium" },
  { path: "machine/z-axis", title: "Set Z-Axis", Component: ZAxisPage, size: "medium" },
  { path: "machine/z-limits", title: "Set Z-Axis Limits", Component: ZLimitsPage, size: "medium" },
  { path: "machine/fake-servo", title: "Fake Servo", Component: FakeServoPage, size: "medium" },
];

/** Old section addresses, kept working: each lands on the modal that replaced it, or the control screen. */
export const legacyRedirects: Record<string, string> = {
  files: "/",
  boards: "/",
  machine: "/actions",
  system: "/actions",
  "system/pid": "/pid",
  "system/about": "/about",
  "system/updates": "/updates",
  "system/logs": "/logs",
};
