import { useClient, useFetch, useMachine } from "../../app/context";
import { useCloseModal } from "../../app/modalHistory";
import { getGcodeLibrary, openGcode } from "./api";
import { useResource } from "./hooks";
import { OpenFromLibrary } from "./LibraryForms";
import { selectGcodeLocked } from "../../state/machine";
import { GCODE_LOCKED_NOTE, GcodeLoadingNote, Loaded, SubPage } from "./ui";

export const OpenGcode = () => {
  const fetchImpl = useFetch();
  const client = useClient();
  const close = useCloseModal();
  const { resource } = useResource(getGcodeLibrary);
  const locked = useMachine(selectGcodeLocked);

  return (
    <SubPage>
      <Loaded resource={resource} what="the gcode library">
        {(library) => (
          <OpenFromLibrary
            library={library}
            legend="Gcode files"
            lockedReason={locked ? GCODE_LOCKED_NOTE : undefined}
            busyNote={<GcodeLoadingNote />}
            open={async (file) => {
              await openGcode(fetchImpl, file);
              client.emit("checkForGCodeUpdate", { data: "Please" });
              close();
            }}
          />
        )}
      </Loaded>
    </SubPage>
  );
};
