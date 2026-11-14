import type { OpticalSettings } from "../../protocol";
import { parseNumber } from "./measurements";

export type OpticalKey = keyof OpticalSettings;
export type OpticalForm = Record<OpticalKey, string>;
export type CornerKey = "tlX" | "tlY" | "brX" | "brY";

export interface NumberField {
  key: Exclude<OpticalKey, "calibrationExtents" | "autoScanDirection" | "gaussianBlurValue">;
  label: string;
  hint: string;
  /** Inclusive integer range; fields without one take any number. */
  integer?: { min: number; max: number };
}

export const CONFIGURATION_FIELDS: NumberField[] = [
  { key: "markerX", label: "Horizontal size of marker", hint: "Inches" },
  { key: "markerY", label: "Vertical size of marker", hint: "Inches" },
  { key: "scaleX", label: "Horizontal scale factor", hint: "Something like 1.0028" },
  { key: "scaleY", label: "Vertical scale factor", hint: "Something like 1.0028" },
  { key: "opticalCenterX", label: "X coordinate of center", hint: "Pixels, something like 300" },
  { key: "opticalCenterY", label: "Y coordinate of center", hint: "Pixels, something like 200" },
  { key: "positionTolerance", label: "Position tolerance", hint: "Millimetres, something like 0.125" },
];

const horizontal = { min: -15, max: 15 };
const vertical = { min: -7, max: 7 };

export const CORNER_FIELDS: NumberField[] = [
  { key: "tlX", label: "X of top left", hint: "-15 to 15 (+ is right)", integer: horizontal },
  { key: "tlY", label: "Y of top left", hint: "-7 to 7 (+ is up)", integer: vertical },
  { key: "brX", label: "X of bottom right", hint: "-15 to 15 (+ is right)", integer: horizontal },
  { key: "brY", label: "Y of bottom right", hint: "-7 to 7 (+ is up)", integer: vertical },
];

export const RECOGNITION_FIELDS: NumberField[] = [
  { key: "cannyLowValue", label: "Canny low value", hint: "0 to 255" },
  { key: "cannyHighValue", label: "Canny high value", hint: "0 to 255" },
];

type Corners = Record<CornerKey, number>;

export const EXTENTS: { value: string; label: string; corners?: Corners }[] = [
  { value: "Top Left", label: "48 x 36 top left", corners: { tlX: -15, tlY: 7, brX: 0, brY: 0 } },
  { value: "Bottom Left", label: "48 x 36 bottom left", corners: { tlX: -15, tlY: 0, brX: 0, brY: -7 } },
  { value: "Top Right", label: "48 x 36 top right", corners: { tlX: 0, tlY: 7, brX: 15, brY: 0 } },
  { value: "Bottom Right", label: "48 x 36 bottom right", corners: { tlX: 0, tlY: 0, brX: 15, brY: -7 } },
  { value: "Full Area", label: "96 x 48 full area", corners: { tlX: -15, tlY: 7, brX: 15, brY: -7 } },
  { value: "Custom", label: "Custom" },
];

export const SCAN_DIRECTIONS = [
  { value: "0", label: "Horizontally" },
  { value: "1", label: "Vertically" },
];

export const BLUR_SIZES = ["1", "3", "5", "7", "9", "11"].map((value) => ({ value, label: `${value} x ${value}` }));

export const isCustomExtents = (extents: string) =>
  !EXTENTS.some((option) => option.value === extents && option.corners);

const nearestBlur = (value: number) =>
  BLUR_SIZES.reduce((best, option) =>
    Math.abs(Number(option.value) - value) < Math.abs(Number(best.value) - value) ? option : best,
  ).value;

/** Form text for each setting; values the selects do not offer become Custom extents and the nearest blur size. */
export const toForm = (settings: OpticalSettings): OpticalForm => ({
  ...(Object.fromEntries(Object.entries(settings).map(([key, value]) => [key, String(value)])) as OpticalForm),
  calibrationExtents: EXTENTS.some((option) => option.value === settings.calibrationExtents)
    ? settings.calibrationExtents
    : "Custom",
  gaussianBlurValue: nearestBlur(settings.gaussianBlurValue),
});

/** Choosing a preset fills in its corners, as the old page's select did; Custom leaves them for editing. */
export const chooseExtents = (form: OpticalForm, extents: string): OpticalForm => {
  const corners = EXTENTS.find((option) => option.value === extents)?.corners;
  const filled = corners ? Object.fromEntries(Object.entries(corners).map(([key, value]) => [key, String(value)])) : {};
  return { ...form, ...filled, calibrationExtents: extents };
};

const NUMBER_FIELDS = [...CONFIGURATION_FIELDS, ...CORNER_FIELDS, ...RECOGNITION_FIELDS];

export const fieldLabels = (keys: string[]) =>
  NUMBER_FIELDS.filter((field) => keys.includes(field.key)).map((field) => field.label);

const fieldError = (field: NumberField, value: number | null) => {
  if (value === null) return "Enter a number";
  if (!field.integer) return null;
  const { min, max } = field.integer;
  return Number.isInteger(value) && value >= min && value <= max ? null : `Enter a whole number from ${min} to ${max}`;
};

export type ParsedOptical = { settings: OpticalSettings } | { errors: Partial<Record<OpticalKey, string>> };

export const parseOpticalForm = (form: OpticalForm): ParsedOptical => {
  const numbers = NUMBER_FIELDS.map((field) => ({ field, value: parseNumber(form[field.key]) }));
  const errors = Object.fromEntries(
    numbers.flatMap(({ field, value }) => {
      const error = fieldError(field, value);
      return error ? [[field.key, error]] : [];
    }),
  );
  if (Object.keys(errors).length > 0) return { errors };
  return {
    settings: {
      ...(Object.fromEntries(numbers.map(({ field, value }) => [field.key, value])) as Omit<
        OpticalSettings,
        "calibrationExtents" | "autoScanDirection" | "gaussianBlurValue"
      >),
      calibrationExtents: form.calibrationExtents,
      autoScanDirection: Number(form.autoScanDirection),
      gaussianBlurValue: Number(form.gaussianBlurValue),
    },
  };
};
