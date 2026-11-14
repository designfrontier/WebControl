import { useEffect, useRef } from "react";
import { useMachine } from "../../app/context";

export const ControllerConsole = () => {
  const messages = useMachine((s) => s.controllerMessages);
  const log = useRef<HTMLOListElement>(null);

  useEffect(() => {
    if (log.current) log.current.scrollTop = log.current.scrollHeight;
  }, [messages]);

  return (
    <section className="console" aria-labelledby="console-title">
      <h2 id="console-title">Controller Messages</h2>
      <ol ref={log} role="log" aria-labelledby="console-title" tabIndex={0}>
        {messages.length === 0 ? (
          <li className="muted">No messages yet.</li>
        ) : (
          messages.map((message, index) => <li key={index}>{message}</li>)
        )}
      </ol>
    </section>
  );
};
