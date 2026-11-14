import { useEffect, useEffectEvent } from "react";
import type { RefObject } from "react";

const FOCUSABLE =
  "button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), a[href]";

/** Keeps Tab inside `container` by wrapping from either end, or pulling focus back in from outside. */
export const trapTab = (event: KeyboardEvent, container: HTMLElement) => {
  const focusable = Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE));
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  const inside = container.contains(document.activeElement);
  const target = event.shiftKey
    ? !inside || document.activeElement === first
      ? last
      : null
    : !inside || document.activeElement === last
      ? first
      : null;
  if (!target) return;
  event.preventDefault();
  target.focus();
};

/** Marks every element beside the path from `element` up to the app's dialog host inert; returns the undo. */
const inertSurroundings = (element: HTMLElement) => {
  const boundary = element.closest(".dialog-host") ?? document.body;
  const ancestors = (node: HTMLElement): HTMLElement[] =>
    node === boundary || !node.parentElement ? [] : [node, ...ancestors(node.parentElement)];
  const siblings = ancestors(element).flatMap((node) =>
    Array.from(node.parentElement?.children ?? []).filter(
      (sibling) => sibling !== node && !sibling.hasAttribute("inert"),
    ),
  );
  siblings.forEach((sibling) => sibling.setAttribute("inert", ""));
  return () => siblings.forEach((sibling) => sibling.removeAttribute("inert"));
};

/**
 * Makes the page around `backdrop` inert and handles Escape and Tab for the dialog `panel` inside it.
 * Stands aside while another modal has made this one inert.
 */
export const useModal = (
  backdrop: RefObject<HTMLElement | null>,
  panel: RefObject<HTMLElement | null>,
  onClose: () => void,
) => {
  const onKeyDown = useEffectEvent((event: KeyboardEvent) => {
    if (!panel.current || panel.current.closest("[inert]")) return;
    if (event.key === "Escape") onClose();
    if (event.key === "Tab") trapTab(event, panel.current);
  });

  useEffect(() => {
    const undo = backdrop.current ? inertSurroundings(backdrop.current) : undefined;
    const listener = (event: KeyboardEvent) => onKeyDown(event);
    document.addEventListener("keydown", listener);
    return () => {
      document.removeEventListener("keydown", listener);
      undo?.();
    };
  }, [backdrop]);
};
