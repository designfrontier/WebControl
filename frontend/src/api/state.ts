import { startupStateSchema } from "../protocol";
import { getJson } from "./http";
import type { Fetch } from "./http";

export const getStartupState = (fetchImpl: Fetch) => getJson(fetchImpl, "/state", startupStateSchema);
