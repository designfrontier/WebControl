import { useEffect, useRef } from "react";
import { useMachine } from "../../app/context";

export const ControllerConsole = () => {
  const messages = useMachine((s) => s.controllerMessages);
  const log = useRef<HTMLOListElement>(null);

  useEffect(() => {
    if (log.current) log.current.scrollTop = log.current.scrollHeight;
  }, [messages]);

  return (
    <section className="panel console" aria-labelledby="console-title">
      <h2 id="console-title">Machine messages</h2>
      {messages.length === 0 ? (
        <p className="muted">No messages yet.</p>
      ) : (
        <ol ref={log} role="log" aria-labelledby="console-title">
          {messages.map((message, index) => (
            <li key={index}>{message}</li>
          ))}
        </ol>
      )}
    </section>
  );
};
