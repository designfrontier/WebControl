import type { ReactNode } from "react";
import { Link } from "react-router";
import { useMachine } from "../../app/context";

interface Props {
  title: string;
  children: ReactNode;
}

/** One machine setup screen under the section's "Machine" heading: a link back to the list, its title and content. */
export const Screen = ({ title, children }: Props) => (
  <section className="machine-screen" aria-labelledby="machine-screen-title">
    <p className="crumb">
      <Link to="/machine">← All machine setup</Link>
    </p>
    <h2 id="machine-screen-title">{title}</h2>
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
