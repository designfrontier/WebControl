import { getJson, sendJson } from "../../api/http";
import type { Fetch } from "../../api/http";
import { machineSetupSchema, quickConfigureSchema } from "../../protocol";
import type { QuickConfigure } from "../../protocol";

export const getMachineSetup = (fetchImpl: Fetch) => getJson(fetchImpl, "/machine/setup", machineSetupSchema);

export const getQuickConfigure = (fetchImpl: Fetch) =>
  getJson(fetchImpl, "/machine/quick-configure", quickConfigureSchema);

export const saveQuickConfigure = (fetchImpl: Fetch, body: QuickConfigure) =>
  sendJson(fetchImpl, "POST", "/machine/quick-configure", body, quickConfigureSchema);
