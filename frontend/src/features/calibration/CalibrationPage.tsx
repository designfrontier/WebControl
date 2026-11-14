import { Link, NavLink, Route, Routes } from "react-router";
import { HoleyCalibration } from "./HoleyCalibration";
import { OpticalCalibration } from "./OpticalCalibration";
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
  {
    path: "optical",
    label: "Optical",
    summary: "Measure positioning error across the sheet with a camera on the sled and send a correction matrix.",
    Component: OpticalCalibration,
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

export const CalibrationPage = () => (
  <section className="calibration" aria-labelledby="calibration-title">
    <h1 id="calibration-title">Calibration</h1>
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
  </section>
);
