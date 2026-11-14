import type { MachineSetup, QuickConfigure } from "../../protocol";

/** Test data for the machine screens, shaped like the /api/machine responses. */
export const machineSetup = (over: Partial<MachineSetup> = {}): MachineSetup => ({
  chainExtendLength: 1650,
  fourMotor: false,
  firmwareSupportsZaxisLimit: true,
  touchPlate: false,
  controllerFirmwareVersion: 1.3,
  ...over,
});

export const quickConfigureValues = (over: Partial<QuickConfigure> = {}): QuickConfigure => ({
  kinematicsType: "Ring",
  rotationRadius: 138.4,
  chainOverSprocket: "Top",
  motorSpacingX: 2978.4,
  motorOffsetY: 463,
  ...over,
});
