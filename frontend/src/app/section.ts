import type { ComponentType } from "react";

/** One top-level screen: its route (relative to /next/, "" for the index), nav label and page component. */
export interface Section {
  path: string;
  label: string;
  Component: ComponentType;
}
