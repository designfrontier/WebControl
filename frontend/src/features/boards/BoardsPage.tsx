import { Navigate, Route, Routes } from "react-router";
import { BoardsHome } from "./BoardsHome";
import { EditBoard } from "./EditBoard";
import { OpenBoard, SaveBoard } from "./OpenSaveBoard";
import { TrimBoard } from "./TrimBoard";
import "../files/files.css";

export const BoardsPage = () => (
  <div className="feature-page fb">
    <h1>Boards</h1>
    <Routes>
      <Route index element={<BoardsHome />} />
      <Route path="open" element={<OpenBoard />} />
      <Route path="save" element={<SaveBoard />} />
      <Route path="edit" element={<EditBoard />} />
      <Route path="trim" element={<TrimBoard />} />
      <Route path="*" element={<Navigate to="/boards" replace />} />
    </Routes>
  </div>
);
