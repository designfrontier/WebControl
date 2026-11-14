import { z } from "zod";
import type { EventSchemas, MessageSchemas } from "./core";

/** Server -> client commands for machine setup: chains, sprockets, z-axis, PID and firmware. */
export const machineMessages = {
  updatePIDData: z.object({ command: z.string(), data: z.unknown() }),
} satisfies MessageSchemas;

export const machineActions = [
  "reportSettings",
  "resetChainLengths",
  "moveToDefault",
  "adjustChain",
  "testMotors",
  "wipeEEPROM",
  "setSprockets",
  "rotateSprocket",
  "setSprocketAutomatic",
  "setSprocketsZero",
  "setSprocketsDefault",
  "setFakeServo",
  "upgradeCustomFirmware",
  "upgradeStockFirmware",
  "upgradeHoleyFirmware",
  "executeVelocityPIDTest",
  "executePositionPIDTest",
] as const;

export const machineEvents = {} satisfies EventSchemas;
