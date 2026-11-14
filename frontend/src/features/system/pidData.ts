import { pidResultSchema } from "../../protocol";
import type { PidKind, PidResult } from "../../protocol";

export interface Series {
  name: string;
  values: number[];
}

export interface PidChart {
  kind: PidKind;
  series: Series[];
}

/** Version 2 firmware reports one comma-separated sample per line; version 1 reports a bare error value. */
const V2_COLUMNS: Record<PidKind, string[]> = {
  velocity: ["Setpoint", "Input", "Output"],
  position: ["Setpoint", "Input", "Output", "RPM input", "Voltage"],
};

export const toChart = ({ result, version, data }: PidResult): PidChart => {
  if (version !== "2") return { kind: result, series: [{ name: "Error", values: data.map(Number) }] };
  const rows = data.map((row) => String(row).split(",").map(Number));
  return {
    kind: result,
    series: V2_COLUMNS[result].map((name, column) => ({ name, values: rows.map((row) => row[column] ?? NaN) })),
  };
};

/** updatePIDData wraps the result as `{ command, data }`; older code paths sent `data` as JSON text. */
export const parsePidMessage = ({ data }: { data: unknown }): PidChart | null => {
  const inner = typeof data === "string" ? safeJson(data) : data;
  const parsed = pidResultSchema.safeParse(inner);
  return parsed.success ? toChart(parsed.data) : null;
};

const safeJson = (text: string): unknown => {
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
};
