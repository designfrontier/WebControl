import { useEffect, useState } from "react";
import { ApiError, getJson } from "../../api/http";
import type { Fetch } from "../../api/http";
import { getStartupState } from "../../api/state";
import { useFetch } from "../../app/context";
import {
  holeyParametersSchema,
  opticalCenterSchema,
  opticalFitSchema,
  opticalScanStartedSchema,
  opticalSettingsSavedSchema,
  opticalStateSchema,
  opticalTestImageSchema,
} from "../../protocol";
import type { OpticalCell, OpticalSettings, OpticalState } from "../../protocol";
import { submit } from "./api";
import type { Outcome } from "./api";

const OPTICAL = "/calibration/optical";

export const getOptical = (fetchImpl: Fetch) => getJson(fetchImpl, OPTICAL, opticalStateSchema);

const errorBody = (error: unknown) =>
  error instanceof ApiError && typeof (error.body as { error?: unknown } | undefined)?.error === "string"
    ? (error.body as { error: string }).error
    : "Could not load optical calibration.";

/** getOptical as an Outcome, keeping the server's reason (409 names the setting that turns it on). */
export const loadOptical = (fetchImpl: Fetch): Promise<Outcome<OpticalState>> =>
  getOptical(fetchImpl).then(
    (data) => ({ ok: true, data }),
    (error: unknown) => ({ ok: false, error: errorBody(error) }),
  );

export const OPTICAL_FEATURE = "optical";

/** GET /api/state's experimental features, fetched on every mount so a Settings save shows at the next screen. */
export const useExperimental = (): readonly string[] => {
  const fetchImpl = useFetch();
  const [features, setFeatures] = useState<readonly string[]>([]);
  useEffect(() => {
    let live = true;
    getStartupState(fetchImpl).then(
      (state) => live && setFeatures(state.experimental),
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [fetchImpl]);
  return features;
};

export const saveOpticalSettings = (fetchImpl: Fetch, settings: OpticalSettings) =>
  submit(fetchImpl, `${OPTICAL}/settings`, settings, opticalSettingsSavedSchema, "PUT");

export const startOpticalScan = (fetchImpl: Fetch, settings: OpticalSettings, cell?: OpticalCell) =>
  submit(fetchImpl, `${OPTICAL}/scan`, cell ? { settings, cell } : { settings }, opticalScanStartedSchema);

export const captureTestImage = (fetchImpl: Fetch, settings: OpticalSettings) =>
  submit(fetchImpl, `${OPTICAL}/test-image`, { settings }, opticalTestImageSchema);

export const findOpticalCenter = (fetchImpl: Fetch, settings: OpticalSettings) =>
  submit(fetchImpl, `${OPTICAL}/find-center`, { settings }, opticalCenterSchema);

export const fitOpticalGeometry = (fetchImpl: Fetch) => submit(fetchImpl, `${OPTICAL}/fit`, {}, opticalFitSchema);

export const acceptOpticalGeometry = (fetchImpl: Fetch) =>
  submit(fetchImpl, `${OPTICAL}/accept`, {}, holeyParametersSchema);

export const opticalCsvUrl = `/api${OPTICAL}/csv`;

export const opticalTargetUrl = (paper: "letter" | "a4") => `/api${OPTICAL}/target.pdf?paper=${paper}`;
