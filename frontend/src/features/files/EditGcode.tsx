import { useState } from "react";
import type { FormEvent } from "react";
import { useFetch, useMachine } from "../../app/context";
import { selectGcodeLocked } from "../../state/machine";
import { CloseButton } from "../../app/CloseButton";
import { useCloseModal } from "../../app/modalHistory";
import { editGcode, getGcodeText } from "./api";
import { useResource, useSubmit } from "./hooks";
import { FormError, GcodeEditor, GcodeLoadingNote, Loaded, SubPage } from "./ui";

const EditForm = ({ initial }: { initial: string }) => {
  const fetchImpl = useFetch();
  const close = useCloseModal();
  const [text, setText] = useState(initial);
  const locked = useMachine(selectGcodeLocked);
  const { busy, error, run } = useSubmit();

  const submit = (event: FormEvent) => {
    event.preventDefault();
    return run(async () => {
      await editGcode(fetchImpl, text.trimEnd());
      close();
    }, "Could not apply the edited gcode.");
  };

  return (
    <form onSubmit={submit} noValidate>
      <GcodeEditor
        label="Loaded gcode"
        value={text}
        onChange={setText}
        hint="Line numbers match Go to line; blank lines in the file are dropped. Applying replaces the loaded gcode; use Save as to keep it in a file."
      />
      <FormError message={error} />
      {locked && <p className="muted">A job is running or paused. Stop the job first to apply changes.</p>}
      {busy && <GcodeLoadingNote />}
      <div className="actions">
        <button
          type="submit"
          className="primary"
          disabled={busy || locked || text === initial}
          title={locked ? "Stop the job first" : undefined}
        >
          {busy ? "Applying…" : "Apply changes"}
        </button>
        <CloseButton>Cancel</CloseButton>
      </div>
    </form>
  );
};

export const EditGcode = () => {
  const { resource } = useResource(getGcodeText);
  return (
    <SubPage>
      <Loaded resource={resource} what="the loaded gcode">
        {(text) => <EditForm initial={text} />}
      </Loaded>
    </SubPage>
  );
};
