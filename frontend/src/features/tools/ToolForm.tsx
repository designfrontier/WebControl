import { useId, useState } from "react";
import type { FormEvent } from "react";
import { useFetch } from "../../app/context";
import { TOOL_NUMBER_MAX } from "../../protocol";
import type { Tool, ToolInput } from "../../protocol";
import { addTool, refusalText, updateTool } from "./api";
import { draftOf, toolInput } from "./draft";
import type { Draft } from "./draft";
import "./tools.css";

interface Props {
  /** The values to start from: a library entry or a blank tool. */
  initial: ToolInput;
  /** The number of the library entry this replaces; a new tool when absent. */
  replaces?: number;
  saveLabel?: string;
  onSaved: (tools: Tool[]) => void;
  onCancel: () => void;
}

export const ToolForm = ({ initial, replaces, saveLabel = "Save tool", onSaved, onCancel }: Props) => {
  const fetchImpl = useFetch();
  const id = useId();
  const [draft, setDraft] = useState(() => draftOf(initial));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const field = (key: keyof Draft) => ({
    id: `${id}-${key}`,
    value: draft[key],
    onChange: (event: { target: { value: string } }) => setDraft({ ...draft, [key]: event.target.value }),
  });

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    const input = toolInput(draft);
    if (typeof input === "string") return setError(input);
    setBusy(true);
    setError(null);
    try {
      const saved = await (replaces === undefined ? addTool(fetchImpl, input) : updateTool(fetchImpl, replaces, input));
      onSaved(saved.tools);
    } catch (caught) {
      setError(refusalText(caught, "Could not save the tool. Try again."));
      setBusy(false);
    }
  };

  return (
    <form
      className="tool-form"
      onSubmit={submit}
      aria-label={replaces === undefined ? "New tool" : `Edit tool ${replaces}`}
    >
      <div className="tool-field tool-number">
        <label htmlFor={`${id}-number`}>Tool number</label>
        <input type="number" min={1} max={TOOL_NUMBER_MAX} step={1} inputMode="numeric" {...field("number")} />
      </div>
      <div className="tool-field tool-description">
        <label htmlFor={`${id}-description`}>Description</label>
        <input type="text" maxLength={200} placeholder='1/4" 2 flute end mill' {...field("description")} />
      </div>
      <div className="tool-field">
        <label htmlFor={`${id}-diameter`}>Diameter</label>
        <div className="tool-diameter">
          <input type="number" min={0} step="any" inputMode="decimal" {...field("diameter")} />
          <select aria-label="Diameter units" {...field("units")}>
            <option value="mm">mm</option>
            <option value="in">in</option>
          </select>
        </div>
      </div>
      <div className="tool-field">
        <label htmlFor={`${id}-flutes`}>Flutes</label>
        <input type="number" min={1} max={20} step={1} inputMode="numeric" {...field("flutes")} />
      </div>
      <div className="tool-field tool-notes">
        <label htmlFor={`${id}-notes`}>Notes</label>
        <textarea rows={2} maxLength={1000} {...field("notes")} />
      </div>
      {error && (
        <p role="alert" className="setting-error tool-form-error">
          {error}
        </p>
      )}
      <div className="tool-form-actions">
        <button type="submit" className="primary" disabled={busy}>
          {saveLabel}
        </button>
        <button type="button" onClick={onCancel}>
          Cancel
        </button>
      </div>
    </form>
  );
};
