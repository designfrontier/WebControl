import type { ReactNode } from "react";
import { useMachine } from "../../app/context";

interface Props {
  title: string;
  children: ReactNode;
}

/** One machine setup screen, shown in a modal that supplies its heading. */
export const Screen = ({ title, children }: Props) => (
  <section className="machine-screen" aria-label={title}>
    {children}
  </section>
);

const LOG_LENGTH = 5;

/** The last few controller replies, where B-command answers such as Z limits show up. */
export const ControllerLog = () => {
  const messages = useMachine((s) => s.controllerMessages);
  const recent = messages.slice(-LOG_LENGTH);
  return (
    <section className="controller-log" aria-labelledby="controller-log-title">
      <h3 id="controller-log-title">Controller messages</h3>
      {recent.length === 0 ? (
        <p className="muted">Nothing from the controller yet.</p>
      ) : (
        <ol role="log">
          {recent.map((message, index) => (
            <li key={messages.length - recent.length + index}>{message}</li>
          ))}
        </ol>
      )}
    </section>
  );
};
