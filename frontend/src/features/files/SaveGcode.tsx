import { useFetch } from "../../app/context";
import { useCloseModal } from "../../app/modalHistory";
import { createGcodeDirectory, getGcodeLibrary, saveGcode } from "./api";
import { useResource } from "./hooks";
import { SaveToLibrary } from "./LibraryForms";
import { Loaded, SubPage } from "./ui";

export const SaveGcode = () => {
  const fetchImpl = useFetch();
  const close = useCloseModal();
  const { resource } = useResource(getGcodeLibrary);

  return (
    <SubPage>
      <p className="muted">Writes the loaded gcode, including any edits, to a file in the gcode library.</p>
      <Loaded resource={resource} what="the gcode library">
        {(library) => (
          <SaveToLibrary
            library={library}
            legend="Existing files"
            createDirectory={(name) => createGcodeDirectory(fetchImpl, name)}
            save={async (directory, fileName) => {
              await saveGcode(fetchImpl, directory, fileName);
              close();
            }}
          />
        )}
      </Loaded>
    </SubPage>
  );
};
