import { useNavigate } from "react-router";
import { useClient, useFetch } from "../../app/context";
import { getGcodeLibrary, openGcode } from "./api";
import { useResource } from "./hooks";
import { OpenFromLibrary } from "./LibraryForms";
import { Loaded, SubPage } from "./ui";

export const OpenGcode = () => {
  const fetchImpl = useFetch();
  const client = useClient();
  const navigate = useNavigate();
  const { resource } = useResource(getGcodeLibrary);

  return (
    <SubPage title="Open gcode" back="/files">
      <Loaded resource={resource} what="the gcode library">
        {(library) => (
          <OpenFromLibrary
            library={library}
            legend="Gcode files"
            cancelTo="/files"
            open={async (file) => {
              await openGcode(fetchImpl, file);
              client.emit("checkForGCodeUpdate", { data: "Please" });
              navigate("/");
            }}
          />
        )}
      </Loaded>
    </SubPage>
  );
};
