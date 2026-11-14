import { useState } from "react";
import { useMachine } from "../../app/context";
import { Dialog } from "./Dialog";

/** The classic UI's deliberate speed bump for enabling fake servo. */
export const FAKE_SERVO_CODE = "42";

interface Props {
  onClose: () => void;
  onSet: (enabled: boolean) => void;
}

export const FakeServoDialog = ({ onClose, onSet }: Props) => {
  const fakeServo = useMachine((s) => s.controller.fakeServo);
  const [code, setCode] = useState("");

  return (
    <Dialog
      title="Fake servo"
      onClose={onClose}
      actions={
        <>
          <button type="button" onClick={onClose}>
            Cancel
          </button>
          <button type="button" onClick={() => onSet(false)}>
            Disable fake servo
          </button>
          <button
            type="button"
            className="button-danger"
            disabled={code.trim() !== FAKE_SERVO_CODE}
            onClick={() => onSet(true)}
          >
            Enable fake servo
          </button>
        </>
      }
    >
      <p>Fake servo is {fakeServo ? "on" : "off"}.</p>
      <p>If fake servo is reported as enabled and you did not mean it to be, disable it.</p>
      <label className="field">
        Secret code to enable
        <input inputMode="numeric" autoComplete="off" value={code} onChange={(event) => setCode(event.target.value)} />
      </label>
      <p className="muted">If you don’t know the code, you almost certainly don’t want fake servo.</p>
    </Dialog>
  );
};
