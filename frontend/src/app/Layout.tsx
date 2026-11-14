import { useState } from "react";
import { Link, NavLink, Outlet } from "react-router";
import { ServerDialogs } from "../features/main";
import { sections } from "./sections";
import { AlarmBanner, StatusBar } from "./StatusBar";

/** One responsive shell: the nav is a row on wide screens and a toggled drawer on phones (see app.css). */
export const Layout = () => {
  const [menuOpen, setMenuOpen] = useState(false);

  return (
    <div className="shell">
      <header className="topbar">
        <Link className="brand" to="/">
          WebControl
        </Link>
        <button
          type="button"
          className="menu-toggle"
          aria-expanded={menuOpen}
          aria-controls="primary-nav"
          onClick={() => setMenuOpen((open) => !open)}
        >
          Menu
        </button>
        <nav id="primary-nav" className="nav" data-open={menuOpen} aria-label="Primary">
          <ul>
            {sections.map(({ path, label }) => (
              <li key={label}>
                <NavLink to={`/${path}`} end={path === ""} onClick={() => setMenuOpen(false)}>
                  {label}
                </NavLink>
              </li>
            ))}
            <li>
              <a href="/">Classic UI</a>
            </li>
          </ul>
        </nav>
        <StatusBar />
      </header>
      <AlarmBanner />
      <main className="content">
        <Outlet />
      </main>
      <ServerDialogs />
    </div>
  );
};
