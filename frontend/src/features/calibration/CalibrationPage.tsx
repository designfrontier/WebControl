import { Link, NavLink, Route, Routes } from "react-router";
import { StopButton } from "../../app/StopButton";
import { AlarmBanner } from "../../app/StatusBar";
import { HoleyCalibration } from "./HoleyCalibration";
import { TriangularCalibration } from "./TriangularCalibration";
import "./calibration.css";

const METHODS = [
  {
    path: "triangular",
    label: "Triangular",
    summary: "Cut five marks and measure them to set the motor height, rotation radius and chain sag.",
    Component: TriangularCalibration,
  },
  {
    path: "holey",
    label: "Holey",
    summary: "For holey firmware: cut six holes and measure twelve distances to set motor spacing and chain tolerance.",
    Component: HoleyCalibration,
  },
];

const Overview = () => (
  <ul className="method-list">
    {METHODS.map(({ path, label, summary }) => (
      <li key={path} className="card">
        <h2>{label}</h2>
        <p>{summary}</p>
        <Link to={path}>Open {label.toLowerCase()} calibration</Link>
      </li>
    ))}
  </ul>
);

/** A full page of its own, unlike every other tool: calibration runs long and needs the room. */
export const CalibrationPage = () => (
  <div className="page">
    <header className="page-header">
      <Link className="button-link" to="/">
        ← Back to Control
      </Link>
      <h1 id="calibration-title">Calibration</h1>
      <StopButton label="Stop machine" />
    </header>
    <AlarmBanner />
    <main className="calibration page-content" aria-labelledby="calibration-title">
      <nav className="subnav" aria-label="Calibration methods">
        <ul>
          {METHODS.map(({ path, label }) => (
            <li key={path}>
              <NavLink to={`/calibration/${path}`}>{label}</NavLink>
            </li>
          ))}
        </ul>
      </nav>
      <Routes>
        <Route index element={<Overview />} />
        {METHODS.map(({ path, Component }) => (
          <Route key={path} path={path} element={<Component />} />
        ))}
        <Route path="*" element={<p>There is no calibration method at this address.</p>} />
      </Routes>
    </main>
  </div>
);
