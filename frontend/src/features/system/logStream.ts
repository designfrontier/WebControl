import { useEffect, useReducer } from "react";
import { logMessageSchema } from "../../protocol";
import type { LogMessage } from "../../protocol";
import type { Transport as SocketLike } from "../../transport";

export { connectLogSocket } from "../../socket/connect";

export const LOG_LINE_LIMIT = 1000;

export type LogName = "log" | "alog";

export interface LogView {
  connected: boolean;
  /** null until LogStreamer reports; true when logging is suspended for user idle time. */
  suspended: boolean | null;
  lines: Record<LogName, string[]>;
  following: Record<LogName, boolean>;
}

export type LogEvent =
  | { type: "connected"; connected: boolean }
  | { type: "message"; message: LogMessage }
  | { type: "follow"; log: LogName; following: boolean }
  | { type: "clear"; log: LogName };

export const initialLogView: LogView = {
  connected: false,
  suspended: null,
  lines: { log: [], alog: [] },
  following: { log: true, alog: true },
};

const append = (lines: string[], line: string) => [...lines, line].slice(-LOG_LINE_LIMIT);

export const logReducer = (view: LogView, event: LogEvent): LogView => {
  switch (event.type) {
    case "connected":
      return { ...view, connected: event.connected };
    case "follow":
      return { ...view, following: { ...view.following, [event.log]: event.following } };
    case "clear":
      return { ...view, lines: { ...view.lines, [event.log]: [] } };
    case "message": {
      const { message } = event;
      const withState = { ...view, suspended: message.state };
      if (message.log === "state" || !view.following[message.log]) return withState;
      return {
        ...withState,
        lines: { ...view.lines, [message.log]: append(view.lines[message.log], message.data.replace(/\r?\n$/, "")) },
      };
    }
  }
};

export const useLogStream = (connect: () => SocketLike) => {
  const [view, dispatch] = useReducer(logReducer, initialLogView);

  useEffect(() => {
    const socket = connect();
    socket.on("connect", () => dispatch({ type: "connected", connected: true }));
    socket.on("disconnect", () => dispatch({ type: "connected", connected: false }));
    socket.on("message", (raw: unknown) => {
      const parsed = logMessageSchema.safeParse(raw);
      if (parsed.success) dispatch({ type: "message", message: parsed.data });
    });
    dispatch({ type: "connected", connected: socket.connected });
    return () => {
      socket.disconnect();
    };
  }, [connect]);

  return [view, dispatch] as const;
};
