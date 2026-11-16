import { decimalPoint } from "../../numbers/decimal";
import type { KinematicsType, QuickConfigure } from "../../protocol";

export const KINEMATICS_OPTIONS: { value: KinematicsType; label: string; image?: string }[] = [
  { value: "Ring", label: "Ring", image: "ring.png" },
  { value: "Linkage", label: "45 degree linkage", image: "linkage.png" },
  { value: "Pantograph", label: "Top mounted pantograph", image: "pantograph.png" },
  { value: "Metal Maslow", label: "Metal Maslow linkage", image: "metalMaslow.png" },
  { value: "Custom", label: "Custom" },
  { value: "Quadrilateral", label: "Quadrilateral", image: "quadrilateral.png" },
];

/** Rotation radius (mm) each kit has; Custom keeps whatever was typed. */
export const KIT_RADIUS: Partial<Record<KinematicsType, number>> = {
  Ring: 138.4,
  Linkage: 273,
  Pantograph: 100,
  "Metal Maslow": 180.34,
  Quadrilateral: 0,
};

/** Sprocket centre to centre is the gearbox outside-edge measurement minus this. */
export const GEARBOX_ALLOWANCE_MM = 40.4;

export type QuickConfigureForm = Omit<QuickConfigure, "rotationRadius" | "motorSpacingX" | "motorOffsetY"> & {
  rotationRadius: string;
  motorSpacingX: string;
  motorOffsetY: string;
};

export const NUMBER_FIELDS = ["rotationRadius", "motorSpacingX", "motorOffsetY"] as const;
export type NumberField = (typeof NUMBER_FIELDS)[number];

export const toForm = (values: QuickConfigure): QuickConfigureForm => ({
  ...values,
  rotationRadius: String(values.rotationRadius),
  motorSpacingX: String(values.motorSpacingX),
  motorOffsetY: String(values.motorOffsetY),
});

const parseMm = (text: string) => {
  const trimmed = decimalPoint(text.trim());
  const value = Number(trimmed);
  return trimmed !== "" && Number.isFinite(value) && value >= 0 ? value : null;
};

/** The fields that are not a length of zero or more. */
export const invalidFields = (form: QuickConfigureForm): NumberField[] =>
  NUMBER_FIELDS.filter((field) => parseMm(form[field]) === null);

/** The request body, or null while any length is invalid. */
export const fromForm = (form: QuickConfigureForm): QuickConfigure | null =>
  invalidFields(form).length > 0
    ? null
    : {
        ...form,
        rotationRadius: Number(decimalPoint(form.rotationRadius)),
        motorSpacingX: Number(decimalPoint(form.motorSpacingX)),
        motorOffsetY: Number(decimalPoint(form.motorOffsetY)),
      };

export const withKinematics = (form: QuickConfigureForm, kinematicsType: KinematicsType): QuickConfigureForm => {
  const radius = KIT_RADIUS[kinematicsType];
  return { ...form, kinematicsType, rotationRadius: radius === undefined ? form.rotationRadius : String(radius) };
};

/** Null when the spacing is not a number, or would go below zero. */
export const subtractGearbox = (spacing: string): string | null => {
  const value = parseMm(spacing);
  if (value === null || value < GEARBOX_ALLOWANCE_MM) return null;
  return String(Number((value - GEARBOX_ALLOWANCE_MM).toFixed(2)));
};
