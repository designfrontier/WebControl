import { decimalPoint } from "../../numbers/decimal";
import type { Setting, SettingsSection } from "../../protocol";

export type FieldValue = string | boolean;
export type FormValues = Record<string, FieldValue>;
export type FormErrors = Record<string, string>;
export type Control = "checkbox" | "select" | "port" | "text";

const NUMBER = /^\s*[-+]?(\d+\.?\d*|\.\d+)([eE][-+]?\d+)?\s*$/;
const WHOLE_NUMBER = /^\s*[-+]?\d+\s*$/;

/** The regex alone lets "1e999" through, which Number() turns into Infinity. */
const isFiniteNumber = (value: string) => NUMBER.test(value) && Number.isFinite(Number(value));

export const isChecked = (value: Setting["value"]) => value === true || value === 1 || value === "1" || value === "on";

export const initialValues = (settings: Setting[]): FormValues =>
  Object.fromEntries(
    settings.map((setting) => [
      setting.key,
      setting.type === "bool" ? isChecked(setting.value) : String(setting.value),
    ]),
  );

export const controlFor = (setting: Setting): Control =>
  setting.type === "bool"
    ? "checkbox"
    : setting.key === "COMport"
      ? "port"
      : setting.type === "options" && setting.options
        ? "select"
        : "text";

/** Some schema descriptions carry an escaped "\\n" instead of a newline. */
export const displayDesc = (desc: string) => desc.replaceAll("\\n", "\n");

export const isLocked = (setting: Setting, enableCustom: boolean) => setting.custom && !enableCustom;

/** The ports to offer, keeping the saved one (even unset) so the picker never shows a port that is not saved. */
export const portOptions = (current: string, ports: string[]) =>
  ports.includes(current) ? ports : [current, ...ports];

/** A select's options, plus the saved value when the schema no longer lists it. */
export const selectOptions = (setting: Setting, current: string) => {
  const options = setting.options ?? [];
  return options.includes(current) ? options : [current, ...options];
};

export const optionLabel = (option: string, setting: Setting) =>
  option !== "" ? option : setting.key === "COMport" ? "Not set" : "Unassigned";

/** Blank park settings mean no park position. */
const BLANK_ALLOWED = ["parkX", "parkY"];

const blank = (value: FieldValue | undefined) => typeof value !== "string" || value.trim() === "";

/** Why a setting cannot be turned on with the form as it stands; one already on can still be turned off. */
export const unavailableReason = (setting: Setting, values: FormValues): string | undefined =>
  setting.key === "parkForToolChange" && values[setting.key] !== true && (blank(values.parkX) || blank(values.parkY))
    ? "Set Park Position X and Y first."
    : undefined;

export const validateValue = (setting: Setting, value: FieldValue): string | undefined =>
  typeof value === "boolean" || (BLANK_ALLOWED.includes(setting.key) && value.trim() === "")
    ? undefined
    : setting.type === "int" && !WHOLE_NUMBER.test(value)
      ? "Enter a whole number"
      : setting.numeric && !isFiniteNumber(decimalPoint(value))
        ? "Enter a number"
        : undefined;

/** The values as the server wants them: numeric settings with a decimal comma get a point. */
export const toSend = (section: SettingsSection, values: FormValues): FormValues =>
  Object.fromEntries(
    Object.entries(values).map(([key, value]) => {
      const numeric = section.settings.some((setting) => setting.key === key && setting.numeric);
      return [key, numeric && typeof value === "string" ? decimalPoint(value) : value];
    }),
  );

/** Errors for the editable settings present in `values` (the changes), keyed by setting key. */
export const validate = (section: SettingsSection, values: FormValues): FormErrors =>
  Object.fromEntries(
    section.settings
      .filter((setting) => setting.key in values && !isLocked(setting, section.enableCustom))
      .flatMap((setting) => {
        const error = validateValue(setting, values[setting.key]);
        return error ? [[setting.key, error]] : [];
      }),
  );

/** Only the settings the user changed, trimmed, so the server keeps everything else as saved. */
export const changedValues = (section: SettingsSection, initial: FormValues, values: FormValues): FormValues =>
  Object.fromEntries(
    section.settings
      .filter((setting) => !isLocked(setting, section.enableCustom) && values[setting.key] !== initial[setting.key])
      .map(({ key, numeric }) => {
        const value = values[key];
        return [key, typeof value === "string" && numeric ? value.trim() : value];
      }),
  );
