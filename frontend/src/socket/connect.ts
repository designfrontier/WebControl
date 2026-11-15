import { LOGS_NAMESPACE, NAMESPACE } from "../protocol";
import { connectTransport } from "../transport";
import type { Transport } from "../transport";

/** Opens the connection to the backend this page was served from. */
export const connectSocket = (): Transport => connectTransport(NAMESPACE);

/** Opens a separate connection to LogStreamer's namespace; it is not the /MaslowCNC client. */
export const connectLogSocket = (): Transport => connectTransport(LOGS_NAMESPACE, { separate: true });
