import { io } from "socket.io-client";
import type { ConnectTransport } from "./types";

/** socket.io v4 (EIO=4) to the Flask-SocketIO server this page was served from; its Socket is already a Transport. */
export const connectSocketIo: ConnectTransport = (namespace, { separate = false } = {}) =>
  io(namespace, { path: "/socket.io", reconnectionDelayMax: 5000, ...(separate ? { forceNew: true } : {}) });
