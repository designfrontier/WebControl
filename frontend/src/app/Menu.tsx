import { useEffect, useId, useRef, useState } from "react";
import type { KeyboardEvent } from "react";

export type MenuItem =
  | { label: string; onSelect: () => void; badge?: string; disabledReason?: string }
  | { label: string; href: string; newTab?: boolean };

const KEY_STEPS: Record<string, (index: number, count: number) => number> = {
  ArrowDown: (index, count) => (index + 1) % count,
  ArrowUp: (index, count) => (index - 1 + count) % count,
  Home: () => 0,
  End: (_, count) => count - 1,
};

/** A menu button: the classic navbar dropdowns, with arrow keys, Escape and outside clicks closing it. */
export const Menu = ({ label, items, badge }: { label: string; items: MenuItem[]; badge?: string }) => {
  const [open, setOpen] = useState(false);
  const id = useId();
  const root = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const entries = () => Array.from(root.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? []);

  useEffect(() => {
    if (!open) return;
    entries()[0]?.focus();
    const onPointerDown = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [open]);

  const closeMenu = () => {
    setOpen(false);
    button.current?.focus();
  };

  const onMenuKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape") return closeMenu();
    if (event.key === "Tab") return setOpen(false);
    const step = KEY_STEPS[event.key];
    if (!step) return;
    event.preventDefault();
    const all = entries();
    all[step(all.indexOf(document.activeElement as HTMLElement), all.length)]?.focus();
  };

  return (
    <div className="menu" ref={root}>
      <button
        ref={button}
        type="button"
        className="menu-button"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={id}
        onClick={() => setOpen((current) => !current)}
        onKeyDown={(event) => {
          if (event.key !== "ArrowDown") return;
          event.preventDefault();
          setOpen(true);
        }}
      >
        {label}
        {badge && (
          <span className="badge" aria-hidden="true">
            {badge}
          </span>
        )}
        <span aria-hidden="true"> ▾</span>
      </button>
      {open && (
        <ul id={id} role="menu" aria-label={label} className="menu-list" onKeyDown={onMenuKeyDown}>
          {items.map((item) => (
            <li key={item.label} role="none">
              {"href" in item ? (
                <a
                  role="menuitem"
                  tabIndex={-1}
                  href={item.href}
                  target={item.newTab ? "_blank" : undefined}
                  rel={item.newTab ? "noreferrer" : undefined}
                  onClick={() => setOpen(false)}
                >
                  {item.label}
                </a>
              ) : (
                <button
                  type="button"
                  role="menuitem"
                  tabIndex={-1}
                  aria-disabled={item.disabledReason ? true : undefined}
                  title={item.disabledReason}
                  onClick={() => {
                    if (item.disabledReason) return;
                    // Focus the menu button first, so a dialog this opens restores focus there on close.
                    closeMenu();
                    item.onSelect();
                  }}
                >
                  {item.label}
                  {item.disabledReason && <span className="menu-reason">{item.disabledReason}</span>}
                  {item.badge && (
                    <span className="badge" aria-hidden="true">
                      {item.badge}
                    </span>
                  )}
                </button>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
};
