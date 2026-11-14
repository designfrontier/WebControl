import { useState } from "react";
import type { FormEvent } from "react";
import { Link, useNavigate } from "react-router";
import { useFetch } from "../../app/context";
import { editGcode, getGcodeText } from "./api";
import { useResource, useSubmit } from "./hooks";
import { FormError, GcodeEditor, Loaded, SubPage } from "./ui";

const EditForm = ({ initial }: { initial: string }) => {
  const fetchImpl = useFetch();
  const navigate = useNavigate();
  const [text, setText] = useState(initial);
  const { busy, error, run } = useSubmit();

  const submit = (event: FormEvent) => {
    event.preventDefault();
    return run(async () => {
      await editGcode(fetchImpl, text.trimEnd());
      navigate("/");
    }, "Could not apply the edited gcode.");
  };

  return (
    <form onSubmit={submit} noValidate>
      <GcodeEditor
        label="Loaded gcode"
        value={text}
        onChange={setText}
        hint="Applying replaces the loaded gcode; use Save as to keep it in a file."
      />
      <FormError message={error} />
      <div className="actions">
        <button type="submit" className="primary" disabled={busy || text === initial}>
          {busy ? "Applying…" : "Apply changes"}
        </button>
        <Link className="button" to="/files">
          Cancel
        </Link>
      </div>
    </form>
  );
};

export const EditGcode = () => {
  const { resource } = useResource(getGcodeText);
  return (
    <SubPage title="View or edit gcode" back="/files">
      <Loaded resource={resource} what="the loaded gcode">
        {(text) => <EditForm initial={text} />}
      </Loaded>
    </SubPage>
  );
};
