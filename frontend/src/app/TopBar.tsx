import { useState } from "react";
import { Link, useNavigate } from "react-router";
import { useUpdateAvailable } from "../features/system";
import type { ActionCommand } from "../protocol";
import { selectGcodeLocked } from "../state/machine";
import { useClient, useMachine } from "./context";
import { ConfirmDialog } from "./Dialog";
import { Menu } from "./Menu";
import type { MenuItem } from "./Menu";
import { StatusBar } from "./StatusBar";
import { ThemeToggle } from "./theme";

const ONLINE_HELP = "https://webcontrolcnc.github.io/WebControl/";
const GCODE_LOCKED_REASON = "Stop the job first";

const CONFIRMS = {
  clearGCode: {
    title: "Clear the loaded gcode?",
    confirmLabel: "Clear GCode",
    body: "The toolpath is removed from WebControl. The file in the gcode library is kept.",
  },
  boardClearBoard: {
    title: "Clear the board's cut data?",
    confirmLabel: "Clear Board",
    body: "WebControl forgets which parts of this board have been cut.",
  },
} as const;

/** The classic navbar (templates/base.html): the same menus in the same order, each opening a modal. */
export const TopBar = () => {
  const navigate = useNavigate();
  const client = useClient();
  const updateAvailable = useUpdateAvailable();
  const locked = useMachine(selectGcodeLocked) ? GCODE_LOCKED_REASON : undefined;
  const hasClassicUi = useMachine((state) => state.backend === "python");
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [confirm, setConfirm] = useState<keyof typeof CONFIRMS | null>(null);

  const done = () => setDrawerOpen(false);
  const open = (path: string) => () => {
    done();
    navigate(path);
  };
  const run = (command: ActionCommand) => () => {
    done();
    client.sendAction(command);
  };

  const gcode: MenuItem[] = [
    { label: "Open", onSelect: open("/files/open"), disabledReason: locked },
    { label: "Save", onSelect: open("/files/save") },
    { label: "Upload", onSelect: open("/files/upload"), disabledReason: locked },
    { label: "Clear GCode", onSelect: () => setConfirm("clearGCode"), disabledReason: locked },
    { label: "View/Edit", onSelect: open("/files/edit") },
    { label: "Send Custom GCode", onSelect: open("/files/send") },
  ];
  const settings: MenuItem[] = [
    { label: "Maslow", onSelect: open("/settings") },
    { label: "Advanced", onSelect: open("/settings/advanced") },
    { label: "WebControl", onSelect: open("/settings/webcontrol") },
    { label: "Camera", onSelect: open("/settings/camera") },
    { label: "GPIO", onSelect: open("/settings/gpio") },
  ];
  const boards: MenuItem[] = [
    { label: "Open Board", onSelect: open("/boards/open") },
    { label: "Save Board", onSelect: open("/boards/save") },
    { label: "Create/Edit Board", onSelect: open("/boards/edit") },
    { label: "Trim Board", onSelect: open("/boards/trim") },
    { label: "Process GCode", onSelect: run("boardProcessGCode") },
    { label: "Clear Board", onSelect: () => setConfirm("boardClearBoard") },
  ];
  const help: MenuItem[] = [
    { label: "Getting Started", onSelect: open("/machine/getting-started") },
    { label: "Online Help", href: ONLINE_HELP, newTab: true },
    { label: "Offline Help", onSelect: open("/help") },
    { label: "About", onSelect: open("/about") },
    { label: "Update", onSelect: open("/updates"), badge: updateAvailable ? "New" : undefined },
    { label: "Logs", href: "/next/logs", newTab: true },
  ];

  return (
    <header className="topbar">
      <Link className="brand" to="/">
        WebControl
      </Link>
      <button
        type="button"
        className="menu-toggle"
        aria-expanded={drawerOpen}
        aria-controls="primary-nav"
        onClick={() => setDrawerOpen((current) => !current)}
      >
        Menu
      </button>
      <nav id="primary-nav" className="nav" data-open={drawerOpen} aria-label="Primary">
        <Menu label="G-Code" items={gcode} />
        <button type="button" className="menu-button" onClick={open("/actions")}>
          Actions
        </button>
        <Menu label="Settings" items={settings} />
        <Menu label="Boards" items={boards} />
        <Menu label="Help" items={help} badge={updateAvailable ? "New" : undefined} />
      </nav>
      <div className="topbar-end">
        <StatusBar />
        <ThemeToggle />
        {hasClassicUi && (
          <a className="classic-link" href="/">
            Classic UI
          </a>
        )}
      </div>
      {confirm && (
        <ConfirmDialog
          title={CONFIRMS[confirm].title}
          confirmLabel={CONFIRMS[confirm].confirmLabel}
          danger
          onCancel={() => setConfirm(null)}
          onConfirm={() => {
            setConfirm(null);
            run(confirm)();
          }}
        >
          <p>{CONFIRMS[confirm].body}</p>
        </ConfirmDialog>
      )}
    </header>
  );
};
