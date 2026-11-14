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

/** GET /api/machine/setup: the firmware-dependent facts the setup screens need. */
export const machineSetupSchema = z.object({
  chainExtendLength: z.number(),
  fourMotor: z.boolean(),
  firmwareSupportsZaxisLimit: z.boolean(),
  touchPlate: z.boolean(),
  controllerFirmwareVersion: z.number(),
});
export type MachineSetup = z.infer<typeof machineSetupSchema>;

export const kinematicsTypes = ["Ring", "Linkage", "Pantograph", "Metal Maslow", "Custom", "Quadrilateral"] as const;
export const chainOverSprocketValues = ["Top", "Bottom"] as const;

/** GET and POST /api/machine/quick-configure; lengths are mm. */
export const quickConfigureSchema = z.object({
  kinematicsType: z.enum(kinematicsTypes),
  rotationRadius: z.number().nonnegative(),
  chainOverSprocket: z.enum(chainOverSprocketValues),
  motorSpacingX: z.number().nonnegative(),
  motorOffsetY: z.number().nonnegative(),
});
export type QuickConfigure = z.infer<typeof quickConfigureSchema>;
export type KinematicsType = QuickConfigure["kinematicsType"];
