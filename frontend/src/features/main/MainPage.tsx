import { Placeholder } from "../../app/Placeholder";
import { Viewer } from "../viewer";

export const MainPage = () => (
  <div className="main-page">
    <Viewer />
    <Placeholder
      title="Control"
      portedFrom="templates/frontpage3d.html, frontpage3d_mobile*.html, static/scripts/frontpageControls.js"
    />
  </div>
);
