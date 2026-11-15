import { useState } from "react";
import type { FormEvent } from "react";
import { useFetch, useMachine } from "../../app/context";
import { selectGcodeLocked } from "../../state/machine";
import { CloseButton } from "../../app/CloseButton";
import { getCustomGcode, sendGcode } from "./api";
import { useResource, useSubmit } from "./hooks";
import { FormError, GcodeEditor, Loaded, SubPage } from "./ui";

const SendForm = ({ initial }: { initial: string }) => {
  const fetchImpl = useFetch();
  const [text, setText] = useState(initial);
  const [sent, setSent] = useState<number | null>(null);
  const { busy, error, setError, run } = useSubmit();
  const locked = useMachine(selectGcodeLocked);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const gcode = text.trimEnd();
    setSent(null);
    if (!gcode.trim()) return setError("Enter the gcode to send.");
    return run(async () => {
      await sendGcode(fetchImpl, gcode);
      setSent(gcode.split("\n").length);
    }, "Could not send the gcode.");
  };

  return (
    <form onSubmit={submit} noValidate>
      <GcodeEditor
        label="Gcode to send"
        value={text}
        onChange={(next) => {
          setText(next);
          setSent(null);
        }}
        hint="Each line goes straight to the controller."
      />
      {locked && <p className="muted">A job is running or paused. Stop the job first to send gcode.</p>}
      <FormError message={error} />
      <p role="status" className="muted">
        {sent === null ? "" : `Sent ${sent} ${sent === 1 ? "line" : "lines"} to the controller.`}
      </p>
      <div className="actions">
        <button type="submit" className="primary" disabled={busy || locked}>
          {busy ? "Sending…" : "Send"}
        </button>
        <CloseButton>Done</CloseButton>
      </div>
    </form>
  );
};

export const SendGcode = () => {
  const { resource } = useResource(getCustomGcode);
  return (
    <SubPage>
      <Loaded resource={resource} what="the last gcode sent">
        {(text) => <SendForm initial={text} />}
      </Loaded>
    </SubPage>
  );
};
