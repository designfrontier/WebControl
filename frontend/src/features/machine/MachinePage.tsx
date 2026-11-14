import { Link, Navigate, Route, Routes } from "react-router";
import { FakeServoPage } from "./FakeServoPage";
import { GettingStartedPage } from "./GettingStartedPage";
import { QuickConfigurePage } from "./QuickConfigurePage";
import { ResetChainsPage } from "./ResetChainsPage";
import { SprocketsPage } from "./SprocketsPage";
import { ZAxisPage } from "./ZAxisPage";
import { ZLimitsPage } from "./ZLimitsPage";
import "./machine.css";

const SCREENS = [
  {
    path: "getting-started",
    title: "Getting started",
    blurb: "The order to set up a new machine in.",
    Page: GettingStartedPage,
  },
  {
    path: "quick-configure",
    title: "Quick configure",
    blurb: "Kit, chain direction and frame measurements.",
    Page: QuickConfigurePage,
  },
  {
    path: "sprockets",
    title: "Sprockets and chains",
    blurb: "Set the sprockets vertical, define zero, extend the chains.",
    Page: SprocketsPage,
  },
  {
    path: "reset-chains",
    title: "Reset chains",
    blurb: "Put the marked links back after the controller loses the chain lengths.",
    Page: ResetChainsPage,
  },
  { path: "z-axis", title: "Z-axis", blurb: "Raise, lower, zero and touch off the router.", Page: ZAxisPage },
  {
    path: "z-limits",
    title: "Z-axis limits",
    blurb: "Save how far the router may plunge and retract.",
    Page: ZLimitsPage,
  },
  {
    path: "fake-servo",
    title: "Fake servo",
    blurb: "Advanced: run the controller without motors attached.",
    Page: FakeServoPage,
  },
];

const MachineHome = () => (
  <section className="machine-screen" aria-label="Machine setup screens">
    <p>Set up the frame, sprockets, chains and router.</p>
    <ul className="screen-list">
      {SCREENS.map(({ path, title, blurb }) => (
        <li key={path}>
          <Link to={`/machine/${path}`}>{title}</Link>
          <span className="muted">{blurb}</span>
        </li>
      ))}
    </ul>
  </section>
);

export const MachinePage = () => (
  <div className="machine">
    <h1>Machine</h1>
    <Routes>
      <Route index element={<MachineHome />} />
      {SCREENS.map(({ path, Page }) => (
        <Route key={path} path={path} element={<Page />} />
      ))}
      <Route path="*" element={<Navigate to="/machine" replace />} />
    </Routes>
  </div>
);
