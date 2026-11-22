import { useClient, useMachine } from "../../app/context";
import { parkNote } from "../main/park";

/** Park and Set Park Here in the Actions modal; Park waits for a park position. */
export const ParkEntries = ({ allowed, onSent }: { allowed: boolean; onSent: (label: string) => void }) => {
  const client = useClient();
  const park = useMachine((s) => s.park);
  const send = (command: "park" | "definePark", label: string) => () => {
    client.sendAction(command);
    onSent(label);
  };
  return (
    <>
      <button
        type="button"
        disabled={!allowed || park === null}
        aria-describedby="park-note"
        onClick={send("park", "Park")}
      >
        Park
      </button>
      <p id="park-note" className="system-note">
        {parkNote(park)}
      </p>
      <button type="button" disabled={!allowed} onClick={send("definePark", "Set park here")}>
        Set park here
      </button>
    </>
  );
};
