import Markdown from "react-markdown";
import type { Components } from "react-markdown";
import { Link } from "react-router";
import remarkGfm from "remark-gfm";
import { helpAssetUrl } from "./api";
import { helpRoute } from "./helpLinks";

const EXTERNAL = /^[a-z][a-z0-9+.-]*:/i;

const NewTab: Components["a"] = ({ href, children }) => (
  <a href={href} target="_blank" rel="noreferrer">
    {children}
  </a>
);

const docsComponents: Components = {
  a: ({ href = "", children }) => {
    const route = EXTERNAL.test(href) ? null : helpRoute(href);
    if (route) return <Link to={route}>{children}</Link>;
    return EXTERNAL.test(href) ? <NewTab href={href}>{children}</NewTab> : <a href={href}>{children}</a>;
  },
  img: ({ src, alt }) => {
    const source = typeof src === "string" ? src : "";
    return <img src={EXTERNAL.test(source) ? source : helpAssetUrl(source)} alt={alt ?? ""} loading="lazy" />;
  },
};

const notesComponents: Components = { a: NewTab };

/** Renders docs and release notes; raw HTML is dropped, never injected. */
export const MarkdownView = ({ children, docs = false }: { children: string; docs?: boolean }) => (
  <div className="markdown">
    <Markdown remarkPlugins={[remarkGfm]} skipHtml components={docs ? docsComponents : notesComponents}>
      {children}
    </Markdown>
  </div>
);
