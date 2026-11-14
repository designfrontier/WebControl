import { useId, useState } from "react";
import { useClient, useMachine } from "../../app/context";
import { Screen } from "./Screen";

/** The code the old fakeServo dialog asked for, so nobody turns it on by accident. */
const SECRET_CODE = "42";

export const FakeServoPage = () => {
  const client = useClient();
  const fakeServo = useMachine((s) => s.controller.fakeServo);
  const connected = useMachine((s) => s.controller.status === "connected");
  const codeId = useId();
  const [code, setCode] = useState("");
  const [result, setResult] = useState<string | null>(null);

  const enable = () => {
    if (code.trim() !== SECRET_CODE) return setResult("That code is not right, so fake servo stays as it was.");
    client.sendAction("setFakeServo", 1);
    setCode("");
    setResult("Asked the controller to enable fake servo.");
  };

  return (
    <Screen title="Fake servo">
      <p className="muted">
        {connected
          ? `When the controller connected, fake servo was reported ${fakeServo ? "on" : "off"}. This may be out of date.`
          : "The controller is not connected."}
      </p>
      <h3>Disable</h3>
      <p>If fake servo is on and you want it off, disable it here.</p>
      <button
        type="button"
        onClick={() => {
          client.sendAction("setFakeServo", 0);
          setResult("Asked the controller to disable fake servo.");
        }}
      >
        Disable fake servo
      </button>
      <h3>Enable</h3>
      <p>
        To enable fake servo, enter the secret code. If you do not know the code, you most likely do not want fake
        servo.
      </p>
      <form
        className="inline-form"
        onSubmit={(event) => {
          event.preventDefault();
          enable();
        }}
      >
        <label htmlFor={codeId}>Secret code</label>
        <input id={codeId} type="password" inputMode="numeric" value={code} onChange={(e) => setCode(e.target.value)} />
        <button type="submit" className="danger">
          Enable fake servo
        </button>
      </form>
      {result && <p className="muted">{result}</p>}
    </Screen>
  );
};
