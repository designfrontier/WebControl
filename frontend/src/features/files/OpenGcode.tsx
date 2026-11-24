import { useState } from "react";
import { useClient, useFetch, useMachine } from "../../app/context";
import { useCloseModal } from "../../app/modalHistory";
import { deleteGcode, getGcodeLibrary, openGcode } from "./api";
import { GCODE_LOAD, useResource } from "./hooks";
import { OpenFromLibrary } from "./LibraryForms";
import { selectGcodeLocked } from "../../state/machine";
import { queueApi } from "../queue/queue";
import { GCODE_LOCKED_NOTE, GcodeLoadingOverlay, Loaded, SubPage } from "./ui";

export const OpenGcode = () => {
  const fetchImpl = useFetch();
  const client = useClient();
  const close = useCloseModal();
  const { resource } = useResource(getGcodeLibrary);
  const locked = useMachine(selectGcodeLocked);
  const [skipBitChanges, setSkipBitChanges] = useState(false);

  return (
    <SubPage>
      <Loaded resource={resource} what="the gcode library">
        {(library) => (
          <OpenFromLibrary
            library={library}
            legend="Gcode files"
            lockedReason={locked ? GCODE_LOCKED_NOTE : undefined}
            busyNote={<GcodeLoadingOverlay title="Opening G-Code…" />}
            submitOptions={GCODE_LOAD}
            open={async (file, signal) => {
              await openGcode(fetchImpl, file);
              client.emit("checkForGCodeUpdate", { data: "Please" });
              if (!signal.aborted) close();
            }}
            enqueue={async ({ directory, name }) =>
              (await queueApi(fetchImpl).add(`${directory}/${name}`, skipBitChanges)).items.length
            }
            remove={(path) => deleteGcode(fetchImpl, path)}
            enqueueOptions={
              <label className="field-inline">
                <input
                  type="checkbox"
                  checked={skipBitChanges}
                  onChange={(event) => setSkipBitChanges(event.target.checked)}
                />
                Don't stop for bit changes in this file when the queue runs it
              </label>
            }
          />
        )}
      </Loaded>
    </SubPage>
  );
};
