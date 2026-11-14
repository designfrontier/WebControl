import { useClient, useFetch } from "../../app/context";
import { useCloseModal } from "../../app/modalHistory";
import { getGcodeLibrary, openGcode } from "./api";
import { useResource } from "./hooks";
import { OpenFromLibrary } from "./LibraryForms";
import { Loaded, SubPage } from "./ui";

export const OpenGcode = () => {
  const fetchImpl = useFetch();
  const client = useClient();
  const close = useCloseModal();
  const { resource } = useResource(getGcodeLibrary);

  return (
    <SubPage>
      <Loaded resource={resource} what="the gcode library">
        {(library) => (
          <OpenFromLibrary
            library={library}
            legend="Gcode files"
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
