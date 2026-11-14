import { io } from "socket.io-client";
import { NAMESPACE } from "../protocol";
import type { SocketLike } from "./client";

/** Opens the real socket.io v4 connection to the Flask-SocketIO server this page was served from. */
export const connectSocket = (): SocketLike => io(NAMESPACE, { path: "/socket.io", reconnectionDelayMax: 5000 });
