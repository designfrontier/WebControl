import { Navigate, Route, Routes } from "react-router";
import { EditGcode } from "./EditGcode";
import { FilesHome } from "./FilesHome";
import { ImportFile } from "./ImportFile";
import { OpenGcode } from "./OpenGcode";
import { SaveGcode } from "./SaveGcode";
import { SendGcode } from "./SendGcode";
import { UploadGcode } from "./UploadGcode";
import "./files.css";

/** Each task is its own route under /files rather than a modal, so dialogs can never stack. */
export const FilesPage = () => (
  <div className="feature-page fb">
    <h1>Files</h1>
    <Routes>
      <Route index element={<FilesHome />} />
      <Route path="open" element={<OpenGcode />} />
      <Route path="upload" element={<UploadGcode />} />
      <Route path="save" element={<SaveGcode />} />
      <Route path="edit" element={<EditGcode />} />
      <Route path="send" element={<SendGcode />} />
      <Route path="import/:kind" element={<ImportFile />} />
      <Route path="*" element={<Navigate to="/files" replace />} />
    </Routes>
  </div>
);
