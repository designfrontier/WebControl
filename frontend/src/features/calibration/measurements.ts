export interface MeasurementField {
  key: string;
  label: string;
  initial: string;
}

export type Parsed = { values: Record<string, number> } | { errors: Record<string, string> };

/** A finite number from a text field, or null when it is blank or not a number. */
export const parseNumber = (text: string) => {
  const trimmed = text.trim();
  const value = Number(trimmed);
  return trimmed === "" || !Number.isFinite(value) ? null : value;
};

export const parseMeasurements = (fields: readonly MeasurementField[], text: Record<string, string>): Parsed => {
  const parsed = fields.map(({ key }) => [key, parseNumber(text[key] ?? "")] as const);
  const errors = Object.fromEntries(
    parsed.filter(([, value]) => value === null).map(([key]) => [key, "Enter a number"]),
  );
  return Object.keys(errors).length > 0
    ? { errors }
    : { values: Object.fromEntries(parsed.map(([key, value]) => [key, value ?? 0])) };
};

const formatter = new Intl.NumberFormat("en-US", { maximumFractionDigits: 6, useGrouping: false });

export const formatValue = (value: number | undefined) => (value === undefined ? "—" : formatter.format(value));
