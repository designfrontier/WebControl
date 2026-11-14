import { useLocation } from "react-router";

/** Renders the router's current path so tests can assert navigation. */
export const Where = () => <output data-testid="location">{useLocation().pathname}</output>;
