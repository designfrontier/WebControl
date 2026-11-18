import { useLayoutEffect, useRef } from "react";
import type { UIEvent } from "react";
import type { Transport as SocketLike } from "../../transport";
import { LOG_LINE_LIMIT, connectLogSocket, useLogStream } from "./logStream";
import type { LogEvent, LogName, LogView } from "./logStream";

const BOTTOM_SLACK_PX = 4;

const PANES: { log: LogName; title: string }[] = [
  { log: "log", title: "Log" },
  { log: "alog", title: "Abridged log" },
];

const stateText = ({ connected, suspended }: LogView) =>
  !connected
    ? "Not connected to the log stream"
    : suspended === null
      ? "Waiting for status…"
      : suspended
        ? "Logging suspended"
        : "Logging active";

const stateTone = ({ connected, suspended }: LogView) =>
  !connected ? "bad" : suspended === null ? undefined : suspended ? "warn" : "ok";

interface PaneProps {
  log: LogName;
  title: string;
  view: LogView;
  dispatch: (event: LogEvent) => void;
}

const LogPane = ({ log, title, view, dispatch }: PaneProps) => {
  const output = useRef<HTMLPreElement>(null);
  const atBottom = useRef(true);
  const lines = view.lines[log];

  useLayoutEffect(() => {
    if (output.current && atBottom.current) output.current.scrollTop = output.current.scrollHeight;
  }, [lines]);

  const onScroll = ({ currentTarget: pane }: UIEvent<HTMLPreElement>) => {
    atBottom.current = pane.scrollHeight - pane.scrollTop - pane.clientHeight < BOTTOM_SLACK_PX;
  };

  return (
    <section className="log-pane" aria-labelledby={`${log}-title`}>
      <div className="log-pane-head">
        <h3 id={`${log}-title`}>{title}</h3>
        <label className="switch">
          <input
            type="checkbox"
            checked={view.following[log]}
            onChange={(event) => dispatch({ type: "follow", log, following: event.target.checked })}
          />
          Stream new lines
        </label>
        <button type="button" onClick={() => dispatch({ type: "clear", log })}>
          Clear view
        </button>
      </div>
      <pre ref={output} onScroll={onScroll} className="log-output" tabIndex={0} aria-label={`${title} lines`}>
        {lines.join("\n")}
      </pre>
      <p className="muted">
        {`${lines.length} line${lines.length === 1 ? "" : "s"} shown (newest ${LOG_LINE_LIMIT} kept)`}
      </p>
    </section>
  );
};

/** The /logs page: live tails of the full and abridged serial log pushed on the log socket. */
export const LogsPanel = ({ connect = connectLogSocket }: { connect?: () => SocketLike }) => {
  const [view, dispatch] = useLogStream(connect);
  return (
    <section aria-label="Logs" className="system-panel">
      <p className="chip log-state" data-tone={stateTone(view)} role="status">
        {stateText(view)}
      </p>
      <div className="log-panes">
        {PANES.map(({ log, title }) => (
          <LogPane key={log} log={log} title={title} view={view} dispatch={dispatch} />
        ))}
      </div>
    </section>
  );
};
