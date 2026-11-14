import type { z } from "zod";
import { boardsActions, boardsEvents, boardsMessages } from "./boards";
import { calibrationActions, calibrationEvents, calibrationMessages } from "./calibration";
import { filesActions, filesEvents, filesMessages } from "./files";
import { machineActions, machineEvents, machineMessages } from "./machine";
import { mainActions, mainEvents, mainMessages } from "./main";
import { settingsActions, settingsEvents, settingsMessages } from "./settings";
import { systemActions, systemEvents, systemMessages } from "./system";
import { viewerActions, viewerEvents, viewerMessages } from "./viewer";

export * from "./actionArgs";
export * from "./boards";
export * from "./calibration";
export * from "./core";
export * from "./files";
export * from "./http";
export * from "./machine";
export * from "./main";
export * from "./settings";
export * from "./system";
export * from "./viewer";

/** Every server -> client `message` command, keyed by command name. */
export const serverMessages = {
  ...mainMessages,
  ...viewerMessages,
  ...settingsMessages,
  ...filesMessages,
  ...boardsMessages,
  ...machineMessages,
  ...calibrationMessages,
  ...systemMessages,
};
export type ServerMessages = typeof serverMessages;
export type ServerCommand = keyof ServerMessages;
export type MessageData<C extends ServerCommand> = z.output<ServerMessages[C]>;

/** Every command Actions.processAction understands. */
export const actionCommands = [
  ...mainActions,
  ...viewerActions,
  ...settingsActions,
  ...filesActions,
  ...boardsActions,
  ...machineActions,
  ...calibrationActions,
  ...systemActions,
] as const;
export type ActionCommand = (typeof actionCommands)[number];

/** Every client -> server socket event the new UI may emit. */
export const clientEvents = {
  ...mainEvents,
  ...viewerEvents,
  ...settingsEvents,
  ...filesEvents,
  ...boardsEvents,
  ...machineEvents,
  ...calibrationEvents,
  ...systemEvents,
};
export type ClientEvents = { [E in keyof typeof clientEvents]: z.input<(typeof clientEvents)[E]> };
export type ClientEvent = keyof ClientEvents;
