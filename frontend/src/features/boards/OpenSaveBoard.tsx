import { useFetch } from "../../app/context";
import { useCloseModal } from "../../app/modalHistory";
import { useResource } from "../files/hooks";
import { OpenFromLibrary, SaveToLibrary } from "../files/LibraryForms";
import { Loaded, SubPage } from "../files/ui";
import { createBoardDirectory, getBoardLibrary, openBoard, saveBoard } from "./api";

export const OpenBoard = () => {
  const fetchImpl = useFetch();
  const close = useCloseModal();
  const { resource } = useResource(getBoardLibrary);

  return (
    <SubPage>
      <Loaded resource={resource} what="the board library">
        {(library) => (
          <OpenFromLibrary
            library={library}
            legend="Board files"
            open={async (file) => {
              await openBoard(fetchImpl, file);
              close();
            }}
          />
        )}
      </Loaded>
    </SubPage>
  );
};

export const SaveBoard = () => {
  const fetchImpl = useFetch();
  const close = useCloseModal();
  const { resource } = useResource(getBoardLibrary);

  return (
    <SubPage>
      <p className="muted">Saves the board's details and its record of cut areas.</p>
      <Loaded resource={resource} what="the board library">
        {(library) => (
          <SaveToLibrary
            library={library}
            legend="Existing boards"
            createDirectory={(name) => createBoardDirectory(fetchImpl, name)}
            save={async (directory, fileName) => {
              await saveBoard(fetchImpl, directory, fileName);
              close();
            }}
          />
        )}
      </Loaded>
    </SubPage>
  );
};
