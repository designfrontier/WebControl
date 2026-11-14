import type { z } from "zod";
import { getJson } from "../../api/http";
import type { Fetch } from "../../api/http";
import {
  calibrationErrorSchema,
  holeyParametersSchema,
  holeyResultSchema,
  opticalSettingsSavedSchema,
  opticalStateSchema,
  triangularParametersSchema,
  triangularResultSchema,
} from "../../protocol";
import type { OpticalSettings } from "../../protocol";

export type Outcome<T> = { ok: true; data: T } | { ok: false; error: string };

/** Like sendJson, but resolves with the server's `{ error }` message instead of throwing, so forms can show it. */
export const submit = async <S extends z.ZodType>(
  fetchImpl: Fetch,
  method: "POST" | "PUT",
  path: string,
  body: unknown,
  schema: S,
): Promise<Outcome<z.output<S>>> => {
  try {
    const response = await fetchImpl(`/api${path}`, {
      method,
      headers: { Accept: "application/json", "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    const json: unknown = await response.json().catch(() => null);
    if (!response.ok) {
      const failure = calibrationErrorSchema.safeParse(json);
      return { ok: false, error: failure.success ? failure.data.error : `The server answered ${response.status}.` };
    }
    const parsed = schema.safeParse(json);
    return parsed.success
      ? { ok: true, data: parsed.data }
      : { ok: false, error: "The server sent an unexpected reply." };
  } catch {
    return { ok: false, error: "Could not reach the server." };
  }
};

export const getTriangular = (fetchImpl: Fetch) =>
  getJson(fetchImpl, "/calibration/triangular", triangularParametersSchema);

export const calculateTriangular = (fetchImpl: Fetch, measurements: Record<string, number>) =>
  submit(fetchImpl, "POST", "/calibration/triangular", measurements, triangularResultSchema);

export const getHoley = (fetchImpl: Fetch) => getJson(fetchImpl, "/calibration/holey", holeyParametersSchema);

export const calculateHoley = (fetchImpl: Fetch, measurements: number[]) =>
  submit(fetchImpl, "POST", "/calibration/holey", { measurements }, holeyResultSchema);

export const getOptical = (fetchImpl: Fetch) => getJson(fetchImpl, "/calibration/optical", opticalStateSchema);

export const saveOpticalSettings = (fetchImpl: Fetch, settings: OpticalSettings) =>
  submit(fetchImpl, "PUT", "/calibration/optical/settings", settings, opticalSettingsSavedSchema);
