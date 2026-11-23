import { useClient, useMachine } from "../../app/context";
import { macroEntries } from "../main/macros";

/** The four macros in the Actions modal, each by its title; one with no gcode is disabled. */
export const MacroEntries = ({ allowed, onSent }: { allowed: boolean; onSent: (label: string) => void }) => {
  const client = useClient();
  const macros = macroEntries(useMachine((s) => s.macros));
  return (
    <section className="action-group" aria-label="Macros">
      <h3>Macros</h3>
      {macros.map(({ command, title, empty }) => (
        <button
          key={command}
          type="button"
          disabled={!allowed || empty}
          aria-describedby={empty ? "macro-note" : undefined}
          onClick={() => {
            client.sendAction(command);
            onSent(title);
          }}
        >
          {title}
        </button>
      ))}
      <p id="macro-note" className="muted">
        A macro with no gcode is disabled. Set each one's title and gcode in Settings ▸ Maslow.
      </p>
    </section>
  );
};
