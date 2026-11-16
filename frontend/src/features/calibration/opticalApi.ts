import { getJson } from "../../api/http";
import type { Fetch } from "../../api/http";
import {
  holeyParametersSchema,
  opticalCenterSchema,
  opticalFitSchema,
  opticalScanStartedSchema,
  opticalSettingsSavedSchema,
  opticalStateSchema,
  opticalTestImageSchema,
} from "../../protocol";
import type { OpticalCell, OpticalSettings } from "../../protocol";
import { submit } from "./api";

const OPTICAL = "/calibration/optical";

export const getOptical = (fetchImpl: Fetch) => getJson(fetchImpl, OPTICAL, opticalStateSchema);

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
