import { Suspense, lazy } from "react";
import type { ComponentProps } from "react";

const MarkdownView = lazy(() => import("./Markdown").then((module) => ({ default: module.MarkdownView })));

/** react-markdown is most of the bundle, so it loads only when help or release notes are shown. */
export const LazyMarkdown = (props: ComponentProps<typeof MarkdownView>) => (
  <Suspense fallback={<p className="muted">Loading…</p>}>
    <MarkdownView {...props} />
  </Suspense>
);
