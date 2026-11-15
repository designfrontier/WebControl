import { useState } from "react";
import type { FormEvent } from "react";
import { ApiError } from "../../api/http";
import { useClient, useFetch, useMachine, useServerMessage } from "../../app/context";
import { errorBodySchema, settingsRejectionSchema } from "../../protocol";
import { selectGcodeLocked } from "../../state/machine";
import type { SettingsSection } from "../../protocol";
import { saveSettingsSection } from "./api";
import { changedValues, initialValues, isLocked, validate } from "./form";
import type { FieldValue, FormErrors } from "./form";
import { SettingField } from "./SettingField";

interface Props {
  section: SettingsSection;
  onSaved: (section: SettingsSection) => void;
}

type Notice = { tone: "ok" | "bad"; text: string } | null;

const serverReason = (error: Error) =>
  (error instanceof ApiError && errorBodySchema.safeParse(error.body).data?.error) || error.message;

const saveFailure = (error: Error) =>
  error instanceof ApiError && error.status === 400
    ? "The server rejected these values; nothing was saved."
    : `Saving failed: ${serverReason(error)}`;

const serverErrors = (error: Error): FormErrors =>
  error instanceof ApiError && error.status === 400
    ? (settingsRejectionSchema.safeParse(error.body).data?.errors ?? {})
    : {};

export const SettingsForm = ({ section, onSaved }: Props) => {
  const fetchImpl = useFetch();
  const client = useClient();
  const [initial, setInitial] = useState(() => initialValues(section.settings));
  const [values, setValues] = useState(initial);
  const [errors, setErrors] = useState<FormErrors>({});
  const [ports, setPorts] = useState(section.ports ?? []);
  const [saving, setSaving] = useState(false);
  const [notice, setNotice] = useState<Notice>(null);
  const locked = useMachine(selectGcodeLocked);

  useServerMessage("updatePorts", setPorts);

  const changes = changedValues(section, initial, values);
  const dirty = Object.keys(changes).length > 0;
  const lockedCount = section.settings.filter((setting) => isLocked(setting, section.enableCustom)).length;

  const change = (key: string, value: FieldValue) => {
    setValues((current) => ({ ...current, [key]: value }));
    setErrors(({ [key]: _cleared, ...rest }) => rest);
    setNotice(null);
  };

  const discard = () => {
    setValues(initial);
    setErrors({});
    setNotice(null);
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    const found = validate(section, changes);
    setErrors(found);
    if (Object.keys(found).length > 0) return setNotice({ tone: "bad", text: "Fix the highlighted settings." });
    setSaving(true);
    try {
      const saved = await saveSettingsSection(fetchImpl, section.id, changes);
      const fresh = initialValues(saved.settings);
      setInitial(fresh);
      setValues(fresh);
      setNotice({ tone: "ok", text: `${section.title} saved.` });
      onSaved(saved);
    } catch (error) {
      setErrors(serverErrors(error as Error));
      setNotice({ tone: "bad", text: saveFailure(error as Error) });
    } finally {
      setSaving(false);
    }
  };

  return (
    <form className="settings-form" onSubmit={submit} noValidate aria-label={section.title}>
      {lockedCount > 0 && (
        <p className="settings-banner">
          {lockedCount === 1 ? "1 setting is" : `${lockedCount} settings are`} locked until the controller reports
          custom firmware (version 100 or later).
        </p>
      )}
      {locked && (
        <p className="settings-banner">A job is running or paused. Stop the job first to change the settings.</p>
      )}
      <ul className="settings-list">
        {section.settings.map((setting) => (
          <SettingField
            key={setting.key}
            setting={setting}
            value={values[setting.key]}
            error={errors[setting.key]}
            locked={isLocked(setting, section.enableCustom)}
            ports={ports}
            onChange={change}
            onRefreshPorts={() => client.sendAction("updatePorts")}
          />
        ))}
      </ul>
      <div className="settings-actions">
        <button type="submit" className="primary" disabled={!dirty || saving || locked}>
          {saving ? "Saving…" : "Save"}
        </button>
        <button type="button" onClick={discard} disabled={!dirty || saving}>
          Discard changes
        </button>
        <p className="settings-notice" role="status" data-tone={notice?.tone}>
          {notice?.text ?? (dirty ? "Unsaved changes." : "")}
        </p>
      </div>
    </form>
  );
};
