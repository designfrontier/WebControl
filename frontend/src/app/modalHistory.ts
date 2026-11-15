import { createContext, use, useEffect, useState } from "react";
import { useLocation, useNavigate, useNavigationType } from "react-router";

interface ModalLocationState {
  modalDepth?: number;
}

const storedDepth = (state: unknown) => (state as ModalLocationState | null)?.modalDepth;

/**
 * How many history entries a modal sits above the control screen, so closing can go back that far and Back
 * closes it. Kept in history state, which survives reloads; a modal opened by a fresh load or reload has depth 0.
 */
export const useModalClose = (isModal: boolean) => {
  const location = useLocation();
  const navigationType = useNavigationType();
  const navigate = useNavigate();
  const stored = storedDepth(location.state);

  const depthAfter = (previous: number) =>
    !isModal ? 0 : (stored ?? (navigationType === "PUSH" ? previous + 1 : navigationType === "REPLACE" ? previous : 0));
  const [tracked, setTracked] = useState(() => ({ key: location.key, depth: depthAfter(0) }));
  const depth = tracked.key === location.key ? tracked.depth : depthAfter(tracked.depth);
  if (tracked.key !== location.key) setTracked({ key: location.key, depth });

  useEffect(() => {
    if (!isModal || stored === depth) return;
    const { pathname, search, hash } = location;
    const state = { ...(location.state as object | null), modalDepth: depth };
    navigate({ pathname, search, hash }, { replace: true, state });
  }, [isModal, stored, depth, location, navigate]);

  return () => (depth > 0 ? navigate(-depth) : navigate("/", { replace: true }));
};

export const ModalContext = createContext<(() => void) | null>(null);

/** Closes the route modal this component renders in. */
export const useCloseModal = () => {
  const close = use(ModalContext);
  if (!close) throw new Error("useCloseModal used outside a modal");
  return close;
};
