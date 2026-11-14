import { useNavigate } from "react-router";
import { useFetch } from "../../app/context";
import { useResource } from "../files/hooks";
import { OpenFromLibrary, SaveToLibrary } from "../files/LibraryForms";
import { Loaded, SubPage } from "../files/ui";
import { createBoardDirectory, getBoardLibrary, openBoard, saveBoard } from "./api";

export const OpenBoard = () => {
  const fetchImpl = useFetch();
  const navigate = useNavigate();
  const { resource } = useResource(getBoardLibrary);

  return (
    <SubPage title="Open board" back="/boards">
      <Loaded resource={resource} what="the board library">
        {(library) => (
          <OpenFromLibrary
            library={library}
            legend="Board files"
            cancelTo="/boards"
            open={async (file) => {
              await openBoard(fetchImpl, file);
              navigate("/");
            }}
          />
        )}
      </Loaded>
    </SubPage>
  );
};

export const SaveBoard = () => {
  const fetchImpl = useFetch();
  const navigate = useNavigate();
  const { resource } = useResource(getBoardLibrary);

  return (
    <SubPage title="Save board as" back="/boards">
      <p className="muted">Saves the board's details and its record of cut areas.</p>
      <Loaded resource={resource} what="the board library">
        {(library) => (
          <SaveToLibrary
            library={library}
            legend="Existing boards"
            cancelTo="/boards"
            createDirectory={(name) => createBoardDirectory(fetchImpl, name)}
            save={async (directory, fileName) => {
              await saveBoard(fetchImpl, directory, fileName);
              navigate("/boards");
            }}
          />
        )}
      </Loaded>
    </SubPage>
  );
};
